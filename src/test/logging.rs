use once_cell::sync::OnceCell;

static LOG_INIT: OnceCell<()> = OnceCell::new();

pub fn init_log() {
    LOG_INIT.get_or_init(|| {
        crate::logging::init();
    });
}