pub mod models;
pub mod repository;

pub use models::User;
pub use repository::{
    init_pool,
    insert_user,
    get_all_users,
    find_user_by_id,
    create_tables,
    update_timestamps,
};
