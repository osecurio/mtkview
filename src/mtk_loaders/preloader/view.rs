use crate::mtk_loaders::preloader::{
    MTKPL_MAGIC, MTKPreloaderLoader,
    gfh_headers::{MtkGfhHeader, gfh_file_info::GfhFileInfo, gfh_types::GFH_TYPES_C_SRC},
};
use base64::prelude::*;
use binaryninja::binary_view::CustomBinaryView;
use binaryninja::binary_view::CustomBinaryViewType;
use binaryninja::{
    architecture::CoreArchitecture,
    binary_view::{BinaryView, BinaryViewBase},
    data_buffer::DataBuffer,
    platform::Platform,
    section::Section,
    segment::Segment,
    symbol::{Symbol, SymbolType},
    types::{CoreTypeParser, TypeParser},
};
use std::ops::Range;
use tracing::{debug, info};

pub struct MTKPreloaderBinaryViewType;

impl CustomBinaryViewType for MTKPreloaderBinaryViewType {
    type CustomBinaryView = MTKPreloaderBinaryView;

    const NAME: &'static str = "mtkview_pl";
    const LONG_NAME: &'static str = "MTK Preloader";

    fn is_valid_for(&self, data: &BinaryView) -> bool {
        let mut magic = Vec::<u8>::new();

        let magic_b64 = BASE64_STANDARD.encode(MTKPL_MAGIC);
        let data_buf = DataBuffer::from_base64(magic_b64.as_str());
        let offset = if let Some(offset) = data.find_next_data(0x0, data.end(), &data_buf) {
            offset
        } else {
            return false;
        };

        data.read_into_vec(&mut magic, offset, 0x300);
        match GfhFileInfo::load(&magic, 0) {
            Some(_) => true,
            None => false,
        }
    }

    fn create_binary_view(&self, data: &BinaryView) -> Result<Self::CustomBinaryView, ()> {
        debug!("Creating MTKPreloaderBinaryView from MTKPreloaderBinaryViewType");
        match MTKPreloaderBinaryView::new(data) {
            Ok(bv) => Ok(bv),
            Err(_) => {
                debug!("MTKPreloaderBinaryView::new() failure!");
                return Err(());
            }
        }
    }
}

impl CustomBinaryView for MTKPreloaderBinaryView {
    fn initialize(&mut self, view: &BinaryView) -> bool {
        debug!("INIT");
        let default_arch = CoreArchitecture::by_name("armv7").unwrap();
        let default_platform = Platform::by_name("thumb2").unwrap();
        let plat_type_container = default_platform.type_container();
        let type_parser = CoreTypeParser::default();
        let parsed_types = type_parser
            .parse_types_from_source(
                GFH_TYPES_C_SRC,
                "gfh_types.h",
                &default_platform,
                &plat_type_container,
                &[],
                &[],
                "",
            )
            .unwrap();
        view.set_default_arch(&default_arch);
        view.set_default_platform(&default_platform);
        info!("{}", self.mtk_pl_loader);

        for (_name, segment) in self.mtk_pl_loader.get_segments() {
            let new_segment = Segment::builder(segment.mapped_addr_range.clone())
                .parent_backing(segment.file_backing.clone())
                .is_auto(true)
                .flags(segment.mapped_segment_flags);

            view.add_segment(new_segment);
        }

        for (name, section) in self.mtk_pl_loader.get_sections() {
            let mut new_section = Section::builder(
                section.name.clone(),
                Range {
                    start: section.mapped_addr_range.start,
                    end: section.mapped_addr_range.end,
                },
            )
            .is_auto(true);

            if name == ".code.data" {
                new_section = new_section.semantics(binaryninja::section::Semantics::ReadOnlyCode);
            }

            view.add_section(new_section);
        }

        // Setup Entry Point
        let entry_forced_platform = Platform::by_name("armv7").unwrap();
        let entry_point = self.get_entry_point();
        let start_symbol = Symbol::builder(SymbolType::Function, "_start", entry_point)
            .full_name("_start")
            .short_name("_start")
            .create();
        view.add_entry_point_with_platform(entry_point, &entry_forced_platform);
        view.define_user_symbol(&start_symbol);

        // Define User Header Types (MOVE THIS CODE INTO THE SPECIFIC MTK HEADER PARSERS)
        let pt_clone = parsed_types.types.clone();
        for pt in parsed_types.types {
            let Some(type_offset) = self.mtk_pl_loader.get_type_addr(&pt.name.to_string()) else {
                continue;
            };

            // Define GFH COMMON for each header... needs refactor?
            let name = pt.name.to_string();
            view.define_user_type(
                "gfh_common_header",
                &pt_clone
                    .iter()
                    .find(|p| p.name == "gfh_common_header".into())
                    .unwrap()
                    .ty,
            );
            let sym = Symbol::builder(
                SymbolType::Data,
                &name,
                self.mtk_pl_loader.get_image_load_addr() as u64 + type_offset as u64,
            )
            .create();
            view.define_auto_symbol_with_type(&sym, &entry_forced_platform, Some(&*pt.ty));

            // Define actual type header
            let name = pt.name.to_string();
            view.define_user_type(name.clone(), &pt.ty);
            let sym = Symbol::builder(
                SymbolType::Data,
                &name,
                self.mtk_pl_loader.get_image_load_addr() as u64 + type_offset as u64,
            )
            .create();

            view.define_auto_symbol_with_type(&sym, &entry_forced_platform, Some(&*pt.ty));
        }

        true
    }
}

pub struct MTKPreloaderBinaryView {
    inner: binaryninja::rc::Ref<BinaryView>,
    mtk_pl_loader: MTKPreloaderLoader,
}

impl BinaryViewBase for MTKPreloaderBinaryView {
    fn address_size(&self) -> usize {
        4
    }

    fn default_endianness(&self) -> binaryninja::Endianness {
        binaryninja::Endianness::LittleEndian
    }

    fn entry_point(&self) -> u64 {
        self.get_entry_point()
    }
}

impl MTKPreloaderBinaryView {
    fn new(view: &BinaryView) -> Result<Self, ()> {
        let read_buffer = view.read_buffer(0, view.len() as usize).ok_or(())?;
        let mtk_pl_loader = MTKPreloaderLoader::new(read_buffer)?;
        Ok(Self {
            inner: view.to_owned(),
            mtk_pl_loader,
        })
    }

    fn get_entry_point(&self) -> u64 {
        self.mtk_pl_loader.get_entry_point()
    }
}

impl AsRef<BinaryView> for MTKPreloaderBinaryView {
    fn as_ref(&self) -> &BinaryView {
        &self.inner
    }
}
