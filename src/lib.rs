use binaryninja::{
    binary_view::{BinaryViewBase, BinaryViewEventType, BinaryViewExt, register_binary_view_event},
    command::{Command, register_command},
    custom_binary_view::register_view_type,
    settings::Settings,
};
use tracing::{debug, error, info};

//use crate::mtk_loaders::lk::LkMd1RomHookContext;

mod mtk_loaders;
mod mtk_settings;
mod util;

pub(crate) type BinaryViewResult<R> = binaryninja::binary_view::Result<R>;

struct LoadCommand;

impl Command for LoadCommand {
    fn action(&self, view: &binaryninja::binary_view::BinaryView) {
        let Some(pv) = view.parent_view() else {
            info!("Failed to get parent view..");
            return;
        };
        let Some(buf) = pv.read_buffer(0, pv.len() as usize) else {
            info!("Failed to get read buffer..");
            return;
        };
        if let Ok(pl) = mtk_loaders::preloader::MTKPreloaderLoader::new(buf) {
            info!("{pl}");
        } else {
            error!("Failed to load buffer with MTKPreloaderLoader!");
        }
    }
    fn valid(&self, _view: &binaryninja::binary_view::BinaryView) -> bool {
        true
    }
}

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn CorePluginInit() -> bool {
    binaryninja::tracing_init!("mtkview");
    debug!("MTK view initializing..");

    // Register Settings Group
    // Register Setting JSON
    let settings = Settings::new();
    settings.register_group("mtkldr", "MTK Loader");
    //settings.register_setting_json("mtkldr", )

    register_view_type(
        "mtkview_pl",
        "MTK Preloader",
        mtk_loaders::preloader::view::MTKPreloaderBinaryViewType::new,
    );

    register_view_type(
        "mtkview_lk",
        "MTK Little Kernel",
        mtk_loaders::lk::view::MTKLkBinaryViewType::new,
    );

    /*
    let md1rom_hook = LkMd1RomHookContext::new(0, 0);
    register_binary_view_event(
        BinaryViewEventType::BinaryViewInitialAnalysisCompletionEvent,
        md1rom_hook,
    );
    */

    register_command(
        "mtkview\\Print Load Information",
        "Prints load information for the current file.",
        LoadCommand,
    );

    debug!("MTK view initialized.");

    true
}
