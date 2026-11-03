//! Protocol layer of the Rustcord client: data model, gateway connection,
//! REST client and an event-sourced state store.
//!
//! Nothing in here touches the UI; everything is plain data plus async tasks
//! so it can be unit tested in isolation.

pub mod gateway;
pub mod model;
pub mod props;
pub mod rest;
pub mod snowflake;
pub mod store;

pub use gateway::{GatewayCommand, GatewayEvent, GatewayHandle, GatewayStatus};
pub use model::*;
pub use props::ClientProperties;
pub use rest::{Rest, RestError};
pub use snowflake::Snowflake;
pub use store::Store;
