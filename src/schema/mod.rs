pub mod builder;
pub mod context;
pub mod resolvers;
pub mod scalars;

pub use builder::build_schema;
pub use context::SchemaContext;

pub type DynamicSchema = async_graphql::dynamic::Schema;
