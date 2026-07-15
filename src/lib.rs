use binaryninja::{
    binary_view::{BinaryViewBase, BinaryViewExt},
    command::{Command, register_command},
    custom_binary_view::register_view_type,
    settings::Settings,
};
use tracing::{debug, error, info};

//use crate::mtk_loaders::lk::LkMd1RomHookContext;

mod commands;
mod mtk_loaders;
mod mtk_settings;
mod util;

pub(crate) type BinaryViewResult<R> = binaryninja::binary_view::Result<R>;

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn CorePluginInit() -> bool {
    binaryninja::tracing_init!("mtkview");
    debug!("MTKView initializing..");

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

    register_command(
        "mtkview\\Print Preloader Load Information",
        "Prints load information for the current file.",
        commands::LoadCommand,
    );

    register_command(
        "mtkview\\Fastboot Heuristics",
        "Run Fastboot Heuristics",
        commands::FastbootHeuristicCommand,
    );

    debug!("MTK view initialized.");

    true
}
