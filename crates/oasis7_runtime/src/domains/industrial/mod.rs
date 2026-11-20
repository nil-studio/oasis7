//! Industrial bounded context: native factory and recipe admission quotes.
//!
//! The World-owned read-only preview APIs remain unchanged.  They must not
//! install state; future industrial mutations are staged at the shared root
//! transaction rather than independently committed inside this module.

use super::{ActionEnvelope, World, WorldError};

mod factory_quote;
mod recipe_quote;

pub use factory_quote::{BuiltinFactoryStartPreview, NativeBuiltinFactoryQuote};
pub use recipe_quote::{BuiltinRecipeStartPreview, NativeBuiltinRecipeQuote};
