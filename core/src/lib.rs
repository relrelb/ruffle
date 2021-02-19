#![allow(
    renamed_and_removed_lints,
    clippy::unknown_clippy_lints,
    clippy::manual_range_contains,
    clippy::same_item_push,
    clippy::unnecessary_wraps
)]

#[macro_use]
mod display_object;

#[macro_use]
extern crate smallvec;

#[macro_use]
extern crate downcast_rs;

#[macro_use]
mod avm1;
mod avm2;
pub mod bitmap;
mod bounding_box;
mod character;
mod collect;
pub mod color_transform;
pub mod context;
mod drawing;
mod ecma_conversions;
pub mod events;
pub mod focus_tracker;
mod font;
mod html;
mod levels;
mod library;
pub mod loader;
mod player;
mod prelude;
pub mod property_map;
pub mod shape_utils;
pub mod string_utils;
pub mod tag_utils;
mod transform;
mod types;
mod vminterface;
mod xml;

pub mod backend;
pub mod config;
pub mod external;

pub use chrono;
pub use events::PlayerEvent;
pub use indexmap;
pub use player::Player;
pub use swf;
pub use swf::Color;
