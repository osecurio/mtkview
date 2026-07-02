use crate::mtk_loaders::lk::MTKLkLoader;
use crate::mtk_loaders::lk::lk_headers::MtkLkHeader;
use crate::mtk_loaders::lk::lk_types::LkCPlatformTypes;
use binaryninja::binary_view::CustomBinaryView;
use binaryninja::binary_view::CustomBinaryViewType;
use binaryninja::symbol::Symbol;
use binaryninja::symbol::SymbolType;
use binaryninja::{
    architecture::CoreArchitecture,
    binary_view::{BinaryView, BinaryViewBase},
    platform::Platform,
    section::Section,
    segment::Segment,
};
use tracing::{debug, info};

pub struct MTKLkBinaryViewType;

impl CustomBinaryViewType for MTKLkBinaryViewType {
    type CustomBinaryView = MTKLkBinaryView;

    const NAME: &'static str = "mtkview_lk";
    const LONG_NAME: &'static str = "MTK Little Kernel";

    fn create_binary_view(&self, data: &BinaryView) -> Result<Self::CustomBinaryView, ()> {
        debug!("Creating MTKLkBinaryView from MTKLkBinaryViewType");
        match MTKLkBinaryView::new(data) {
            Ok(bv) => Ok(bv),
            Err(_) => {
                debug!("MTKLkBinaryView::new() failure!");
                return Err(());
            }
        }
    }

    fn is_valid_for(&self, data: &BinaryView) -> bool {
        let rawr = data.read_buffer(0, data.len() as usize).unwrap();
        match MtkLkHeader::load(rawr.get_data(), false) {
            Some(_) => true,
            None => false,
        }
    }
}

impl CustomBinaryView for MTKLkBinaryView {
    fn initialize(&mut self, view: &BinaryView) -> bool {
        debug!("INIT");

        let (def_arch, def_plat) = {
            match self.get_mtk_address_size() {
                4 => ("armv7", "thumb2"),
                8 => ("aarch64", "aarch64"),
                _ => return false,
            }
        };
        let default_arch = CoreArchitecture::by_name(def_arch).unwrap();
        let default_platform = Platform::by_name(def_plat).unwrap();

        view.set_default_arch(&default_arch);
        view.set_default_platform(&default_platform);

        info!("{}", self.mtk_lk_loader);
        for (name, section) in self.mtk_lk_loader.get_sections() {
            if !section.is_lk() {
                continue;
            }
            let segmentized = section.get_segmentized();
            let header_new_segment = Segment::builder(segmentized.get_header_mapped_addr_range())
                .parent_backing(segmentized.get_header_file_backing())
                .is_auto(true)
                .flags(segmentized.get_header_mapped_seg_flags());

            view.add_segment(header_new_segment);

            let data_new_segment = Segment::builder(segmentized.get_data_mapped_addr_range())
                .parent_backing(segmentized.get_data_file_backing())
                .is_auto(true)
                .flags(segmentized.get_data_mapped_seg_flags());

            view.add_segment(data_new_segment);

            let mut new_header_section = Section::builder(
                format!("{}_header", name),
                segmentized.get_header_mapped_addr_range(),
            )
            .is_auto(true);
            new_header_section =
                new_header_section.semantics(binaryninja::section::Semantics::ReadOnlyData);
            println!("Attempting to create section: {:#X?}", new_header_section);
            view.add_section(new_header_section);

            let mut new_data_section = Section::builder(
                format!("{}_data", name),
                segmentized.get_data_mapped_addr_range(),
            )
            .is_auto(true);
            new_data_section =
                new_data_section.semantics(binaryninja::section::Semantics::DefaultSection);

            println!("Attempting to create section: {:?}", new_data_section);

            view.add_section(new_data_section);
        }

        // Setup Entry Point
        let entry_forced_platform = Platform::by_name(def_arch).unwrap();
        let entry_point = self.get_entry_point();
        let start_symbol = Symbol::builder(SymbolType::Function, "_start", entry_point)
            .full_name("_start")
            .short_name("_start")
            .create();
        view.add_entry_point_with_platform(entry_point, &entry_forced_platform);
        view.define_user_symbol(&start_symbol);

        // Define User Header Types (MOVE THIS CODE INTO THE SPECIFIC MTK HEADER PARSERS)
        let plat_types = LkCPlatformTypes::new(def_plat);

        let lk_hdr_type = plat_types.get_type_by_name("lk_hdr_32").unwrap();

        let name = lk_hdr_type.name.to_string();
        view.define_user_type("lk_hdr_32", &lk_hdr_type.ty);
        let sym = Symbol::builder(
            SymbolType::Data,
            &name,
            view.section_by_name("lk_header")
                .unwrap()
                .address_range()
                .start,
        )
        .create();
        view.define_auto_symbol_with_type(&sym, &entry_forced_platform, Some(&*lk_hdr_type.ty));
        true
    }
}

pub struct MTKLkBinaryView {
    inner: binaryninja::rc::Ref<BinaryView>,
    mtk_lk_loader: MTKLkLoader,
}

impl BinaryViewBase for MTKLkBinaryView {
    fn address_size(&self) -> usize {
        self.get_mtk_address_size()
    }

    fn default_endianness(&self) -> binaryninja::Endianness {
        binaryninja::Endianness::LittleEndian
    }

    fn entry_point(&self) -> u64 {
        self.get_entry_point()
    }
}

impl MTKLkBinaryView {
    fn new(view: &BinaryView) -> Result<Self, ()> {
        let read_buffer = view.read_buffer(0, view.len() as usize).ok_or(())?;
        let mtk_lk_loader = MTKLkLoader::new(read_buffer.get_data())?;
        Ok(Self {
            inner: view.to_owned(),
            mtk_lk_loader,
        })
    }

    //fn init(view: &BinaryView) -> BinaryViewResult<()> {}

    fn get_entry_point(&self) -> u64 {
        self.mtk_lk_loader.get_entry_point("lk")
    }

    fn get_mtk_address_size(&self) -> usize {
        self.mtk_lk_loader.get_address_size()
    }
}

impl AsRef<BinaryView> for MTKLkBinaryView {
    fn as_ref(&self) -> &BinaryView {
        &self.inner
    }
}
