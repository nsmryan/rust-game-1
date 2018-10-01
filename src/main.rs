extern crate ggez;
extern crate rand;
extern crate warmy;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate splines;
extern crate ncollide2d;
extern crate ezing;

use std::fs::File;
use std::io;
use std::io::{Read};
use std::error::Error;
use std::fmt;
use std::cmp::min;

use splines::*;

use rand::prelude::*;

use warmy::{FSKey, Load, Loaded, Storage, Store, StoreOpt, Res};

use ggez::*;
use ggez::graphics::{DrawMode, Point2, Vector2, Mesh, Drawable};
use ggez::graphics;
use ggez::event::{EventHandler, Keycode, Mod};
use ggez::timer;
use ggez::nalgebra as na;


const GAME_FPS : u32 = 60;


#[derive(Serialize, Deserialize, Debug)]
struct Config {
  pub player_size  : f32,
  pub player_speed : f32,
  pub player_ratio : f32,
  pub player_tol   : f32,
  pub dot_size     : f32,
  pub dot_progress : f32,
  pub num_dots     : usize,
}

impl Config {
    pub fn new() -> Config {
        Config { player_size:  1.0,
                 player_speed: 1.0,
                 player_ratio: 0.9,
                 player_tol:   0.9,
                 dot_size:     1.0,
                 dot_progress: 0.01,
                 num_dots:      30,
        }
    }
}

#[derive(Debug)]
enum ConfigError {
    FileError(io::Error),
    JsonError(serde_json::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::FileError(e) => e.fmt(f),
            ConfigError::JsonError(e) => e.fmt(f),
        }
    }
}

impl Error for ConfigError {
    fn cause(&self) -> Option<&Error> {
        match self {
            ConfigError::FileError(e) => Some(e),
            ConfigError::JsonError(e) => Some(e),
        }
    }
}

impl<C> Load<C> for Config {
    type Key = FSKey;

    type Error = ConfigError;

    fn load(key : Self::Key,
            storage : &mut Storage<C>,
            _ : &mut C
            ) -> Result<Loaded<Self>, Self::Error> {
        let mut file = File::open(key.as_path());
        match file {
            Ok(mut fh) => {
                let mut s = String::new();
                fh.read_to_string(&mut s);
                let maybe_config : serde_json::Result<Config> = serde_json::from_str(s.as_str());

                match maybe_config {
                    Ok(config) => Ok(config.into()),
                    Err(e) => Err(ConfigError::JsonError(e)),
                }
            }

            Err(e) => {
                Err(ConfigError::FileError(e))
            }
        }
    }
}

struct InputState {
    pub xaxis: f32,
    pub yaxis: f32,
    pub jump:  bool,
}

impl InputState {
    pub fn new() -> InputState {
        InputState { xaxis: 0.0, yaxis: 0.0, jump: false }
    }
}

enum Direction {
    Left,
    Right,
}

enum PlayerState {
    Idle,
    Walking(Direction),
    Jumping,
    Falling,
}

impl PlayerState {
    pub fn new() -> PlayerState {
        PlayerState::Idle
    }
}

struct Player {
    pos : Point2,
    state : PlayerState,
}

impl Player {
    pub fn new() -> Player {
        Player { pos: Point2::new(200.0, 500.0),
                 state: PlayerState::new(),
        }
    }
}

struct Level {
    floor : f32,
}

impl Level {
    pub fn new() -> Level {
        Level { floor : 500.0 }
    }
}

struct Dot {
  pos      : Point2,
  points   : Vec<Key<Point2>>,
  anchor   : Point2,
  radius   : f32,
  spline   : Spline<Point2>,
  progress : f32
}

impl Dot {
  pub fn new(anchor : Point2, radius : f32) -> Dot {
    let mut points = Vec::new();
    
    points.push(Key::new(0.0,
                         Point2::new(anchor.x + thread_rng().gen::<f32>() * radius,
                                     anchor.y + thread_rng().gen::<f32>() * radius),
                         Interpolation::Linear));
    points.push(Key::new(0.5,
                         Point2::new(anchor.x + thread_rng().gen::<f32>() * radius,
                                     anchor.y + thread_rng().gen::<f32>() * radius),
                         Interpolation::Linear));
    points.push(Key::new(1.0,
                         Point2::new(anchor.x + thread_rng().gen::<f32>() * radius,
                                     anchor.y + thread_rng().gen::<f32>() * radius),
                         Interpolation::Linear));

    Dot { pos: anchor,
          points: points.clone(),
          anchor: anchor,
          radius: radius,
          spline: Spline::from_vec(points.clone()),
          progress: thread_rng().gen::<f32>(),
    }
  }

  pub fn point(&self) -> Point2 {
     Point2::new(self.anchor.x + thread_rng().gen::<f32>() * self.radius,
                 self.anchor.y + thread_rng().gen::<f32>() * self.radius)
  }
}

struct MainState {
    player    : Player,
    input     : InputState,
    level     : Level,
    config    : Res<Config>,
    store     : Store<()>,
    bg_points : Vec<Point2>,
    rng       : ThreadRng,
    dots      : Vec<Dot>,
}

impl MainState {
    pub fn new(ctx : &mut Context) -> GameResult<MainState> {
        let state =
            MainState { player    : Player::new(),
                        input     : InputState::new(),
                        level     : Level::new(),
                        config    : Res::new(Config::new()),
                        store     : Store::new(StoreOpt::default().set_root(".")).expect("error creating store"),
                        bg_points : Vec::new(),
                        rng       : thread_rng(),
                        dots      : Vec::new(),
            };

        Ok(state)
    }
}

impl EventHandler for MainState {
    fn update(&mut self, ctx: &mut Context) -> GameResult<()> {
      let (width, height) = graphics::get_size(ctx);

      while timer::check_update_time(ctx, GAME_FPS) {
          self.player.pos.x += self.input.xaxis;

          for dot in self.dots.iter_mut() {
            let spline_progress = ezing::sine_inout(dot.progress);
            dot.pos = dot.spline.clamped_sample(spline_progress);
          }
      }

      {
        let config = self.config.borrow();
        for index in 0..(config.num_dots - self.dots.len())  {
          self.dots.push(Dot::new(Point2::new(self.rng.gen::<f32>() * width as f32,
                                              self.rng.gen::<f32>() * height as f32),
                                  50.0));
                                  
        }

        for dot in self.dots.iter_mut() {
          dot.progress = (dot.progress + config.dot_progress).min(1.0);

          if dot.progress >= 1.0 {
            // TODO create new spline with current pos as starting point
            dot.progress = 0.0;
            let mut first  = dot.points[2];
            first.t = 0.0;
            let second = dot.point();
            let third = dot.point();
            dot.points.clear();
            dot.points.push(first);
            dot.points.push(Key::new(0.5, second, Interpolation::Linear));
            dot.points.push(Key::new(1.0, third,  Interpolation::Linear));
            dot.spline = Spline::from_vec(dot.points.clone());
            dot.pos = dot.spline.sample(dot.progress).unwrap();
          }
        }
      }

      self.store.sync(&mut ());

      Ok(())
    }

    fn key_down_event(&mut self, ctx: &mut Context, keycode: Keycode, _keymode: Mod, _repeat: bool) {
        let config = self.config.borrow();

        match keycode {
            Keycode::Left | Keycode::A => {
                self.input.xaxis = -config.player_speed;
            }

            Keycode::Right | Keycode::D => {
                self.input.xaxis = config.player_speed;
            }

            Keycode::Escape | Keycode::Q => {
                ctx.quit().unwrap();
            }

            _ => (),
        }
    }

    fn key_up_event(&mut self, ctx: &mut Context, keycode: Keycode, _keymode: Mod, _repeat: bool) {
        match keycode {
            Keycode::Left | Keycode::A => {
                self.input.xaxis = 0.0;
            }

            Keycode::Right | Keycode::D => {
                self.input.xaxis = 0.0;
            }

            _ => (),
        }
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult<()> {
        graphics::clear(ctx);

        draw_level(self, ctx);
        draw_dots(self, ctx);
        draw_player(self, ctx);

        graphics::present(ctx);

        Ok(())
    }
}

fn draw_dots(state : &mut MainState, ctx: &mut Context) {
  let config = state.config.borrow();

  // NOTE recreating mesh every time instead of using 
  // persistent one.
  let dot_mesh = Mesh::new_circle(ctx,
                                  DrawMode::Fill,
                                  Point2::new(0.0, 0.0),
                                  config.dot_size,
                                  1.0).unwrap();

  graphics::set_color(ctx, graphics::WHITE).unwrap();

  for dot in state.dots.iter() {
    dot_mesh.draw(ctx, dot.pos, 0.0);
  }
}

fn draw_level(state : &mut MainState, ctx: &mut Context) {
    let (width, _) = graphics::get_size(ctx);

    let points = [Point2::new(0.0, state.level.floor), Point2::new(width as f32, state.level.floor)];

    graphics::set_color(ctx, graphics::BLACK).unwrap();
    graphics::line(ctx, &points, 2.0).unwrap();
}

fn draw_player(state : &mut MainState, ctx: &mut Context) {
    let config = state.config.borrow();

    graphics::set_color(ctx, graphics::WHITE).unwrap();
    graphics::circle(ctx,
                     DrawMode::Fill,
                     state.player.pos,
                     config.player_size,
                     config.player_tol).unwrap();

    graphics::set_color(ctx, graphics::BLACK).unwrap();
    graphics::circle(ctx,
                     DrawMode::Fill,
                     state.player.pos,
                     config.player_size * config.player_ratio,
                     config.player_tol).unwrap();

    let left_offset = Vector2::new(config.player_size/1.1,
                                   config.player_size/1.3);
    graphics::set_color(ctx, graphics::WHITE).unwrap();
    graphics::circle(ctx,
                     DrawMode::Fill,
                     state.player.pos + left_offset,
                     config.player_size/2.5,
                     config.player_tol).unwrap();

    let right_offset = Vector2::new(-1.0 * config.player_size/1.1,
                                    config.player_size/1.3);
    graphics::circle(ctx,
                     DrawMode::Fill,
                     state.player.pos + right_offset,
                     config.player_size/2.5,
                     config.player_tol).unwrap();
}

fn main() {
    let c = conf::Conf::new();
    let ctx = &mut Context::load_from_conf("rust-game-1", "ggez", c).unwrap();
    let state = &mut MainState::new(ctx).unwrap();

    let warmy_context = &mut ();

    println!("root = '{:?}'", state.store.root());
    state.config = state.store.get::<_, Config>(&FSKey::new("/config.json"), warmy_context).unwrap();

    event::run(ctx, state);
}
