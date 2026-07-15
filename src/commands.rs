use crate::mtk_loaders::lk::modules::fastboot::{detect_fb_subroutines, populate_fastboot_srs};
use crate::mtk_loaders::preloader::MTKPreloaderLoader;
use binaryninja::binary_view::BinaryViewBase;
use binaryninja::command::Command;
use tracing::{error, info, warn};

pub struct LoadCommand;

impl Command for LoadCommand {
    fn action(&self, view: &binaryninja::binary_view::BinaryView) {
        if view.view_type() != "mtkview_pl" {
            warn!("Not a Preloader!");
            return;
        }

        let Some(pv) = view.parent_view() else {
            info!("Failed to get parent view..");
            return;
        };
        let Some(buf) = pv.read_buffer(0, pv.len() as usize) else {
            info!("Failed to get read buffer..");
            return;
        };
        if let Ok(pl) = MTKPreloaderLoader::new(buf) {
            info!("{pl}");
        } else {
            error!("Failed to load buffer with MTKPreloaderLoader!");
        }
    }
    fn valid(&self, _view: &binaryninja::binary_view::BinaryView) -> bool {
        true
    }
}

pub struct FastbootHeuristicCommand;

impl Command for FastbootHeuristicCommand {
    fn action(&self, view: &binaryninja::binary_view::BinaryView) {
        if view.view_type() != "mtkview_lk" {
            warn!("Not a Little Kernel!");
            return;
        }
        populate_fastboot_srs(view).unwrap();
    }
    fn valid(&self, view: &binaryninja::binary_view::BinaryView) -> bool {
        true
    }
}
