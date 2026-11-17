use std::sync::Once;

static INIT: Once = Once::new();

pub(crate) fn setup() {
    INIT.call_once(|| {
        simple_logger::init_with_level(log::Level::Debug).unwrap();
        dotenvy::dotenv().unwrap();
    });
}
