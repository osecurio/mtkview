use crate::mtk_loaders::lk::MTKLkLoader;
use crate::mtk_loaders::lk::lk_types::LkCPlatformTypes;
use crate::{BinaryViewResult, mtk_loaders::lk::lk_headers::MtkLkHeader};
use binaryninja::symbol::Symbol;
use binaryninja::symbol::SymbolType;
use binaryninja::{
    architecture::CoreArchitecture,
    binary_view::{BinaryView, BinaryViewBase, BinaryViewExt},
    custom_binary_view::{
        BinaryViewType, BinaryViewTypeBase, CustomBinaryView, CustomBinaryViewType,
    },
    platform::Platform,
    section::Section,
    segment::Segment,
};
use tracing::{debug, info};

pub struct MTKLkBinaryViewType {
    view_type: BinaryViewType,
}

impl MTKLkBinaryViewType {
    pub fn new(view_type: BinaryViewType) -> Self {
        Self { view_type }
    }
}

impl AsRef<BinaryViewType> for MTKLkBinaryViewType {
    fn as_ref(&self) -> &BinaryViewType {
        &self.view_type
    }
}

impl BinaryViewTypeBase for MTKLkBinaryViewType {
    fn is_deprecated(&self) -> bool {
        false
    }
    fn is_force_loadable(&self) -> bool {
        false
    }
    fn is_valid_for(&self, data: &BinaryView) -> bool {
        let rawr = data.read_buffer(0, data.len() as usize).unwrap();
        match MtkLkHeader::load(rawr.get_data(), false) {
            Some(_) => true,
            None => false,
        }
    }
}

impl CustomBinaryViewType for MTKLkBinaryViewType {
    fn create_custom_view<'builder>(
        &self,
        data: &BinaryView,
        builder: binaryninja::custom_binary_view::CustomViewBuilder<'builder, Self>,
    ) -> binaryninja::binary_view::Result<binaryninja::custom_binary_view::CustomView<'builder>>
    {
        debug!("Creating MTKLkBinaryView from MTKLkBinaryViewType");

        let bv = builder.create::<MTKLkBinaryView>(data, ());
        bv
    }
}

unsafe impl CustomBinaryView for MTKLkBinaryView {
    type Args = ();

    fn new(handle: &BinaryView, _args: &Self::Args) -> binaryninja::binary_view::Result<Self> {
        MTKLkBinaryView::new(handle)
    }

    fn init(&mut self, _args: Self::Args) -> binaryninja::binary_view::Result<()> {
        MTKLkBinaryView::init(self)
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
    fn new(view: &BinaryView) -> BinaryViewResult<Self> {
        let parent_view = view.parent_view().ok_or(())?;
        let read_buffer = parent_view
            .read_buffer(0, parent_view.len() as usize)
            .ok_or(())?;
        let mtk_lk_loader = MTKLkLoader::new(read_buffer.get_data())?;
        Ok(Self {
            inner: view.to_owned(),
            mtk_lk_loader,
        })
    }

    fn init(&self) -> BinaryViewResult<()> {
        debug!("INIT");

        let (def_arch, def_plat) = {
            match self.get_mtk_address_size() {
                4 => ("armv7", "armv7"),
                8 => ("aarch64", "aarch64"),
                _ => return Err(()),
            }
        };
        let default_arch = CoreArchitecture::by_name(def_arch).ok_or(())?;
        let default_platform = Platform::by_name(def_plat).ok_or(())?;

        self.set_default_arch(&default_arch);
        self.set_default_platform(&default_platform);

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

            self.add_segment(header_new_segment);

            let data_new_segment = Segment::builder(segmentized.get_data_mapped_addr_range())
                .parent_backing(segmentized.get_data_file_backing())
                .is_auto(true)
                .flags(segmentized.get_data_mapped_seg_flags());

            self.add_segment(data_new_segment);

            let mut new_header_section = Section::builder(
                format!("{}_header", name),
                segmentized.get_header_mapped_addr_range(),
            )
            .is_auto(true);
            new_header_section =
                new_header_section.semantics(binaryninja::section::Semantics::ReadOnlyData);
            println!("Attempting to create section: {:#X?}", new_header_section);
            self.add_section(new_header_section);

            let mut new_data_section = Section::builder(
                format!("{}_data", name),
                segmentized.get_data_mapped_addr_range(),
            )
            .is_auto(true);
            new_data_section =
                new_data_section.semantics(binaryninja::section::Semantics::ReadOnlyCode);

            println!("Attempting to create section: {:?}", new_data_section);

            self.add_section(new_data_section);
        }

        // Setup Entry Point
        let entry_forced_platform = Platform::by_name(def_plat).ok_or(())?;
        let entry_point = self.get_entry_point();
        let start_symbol = Symbol::builder(SymbolType::Function, "_start", entry_point)
            .full_name("_start")
            .short_name("_start")
            .create();
        self.add_entry_point_with_platform(entry_point, &entry_forced_platform);
        self.define_user_symbol(&start_symbol);

        // Define User Header Types (MOVE THIS CODE INTO THE SPECIFIC MTK HEADER PARSERS)
        let plat_types = LkCPlatformTypes::new(def_plat);

        let lk_hdr_type = plat_types.get_type_by_name("lk_hdr_32").unwrap();

        let name = lk_hdr_type.name.to_string();
        self.define_user_type("lk_hdr_32", &lk_hdr_type.ty);
        let sym = Symbol::builder(
            SymbolType::Data,
            &name,
            self.section_by_name("lk_header")
                .unwrap()
                .address_range()
                .start,
        )
        .create();
        self.define_auto_symbol_with_type(&sym, &entry_forced_platform, Some(&*lk_hdr_type.ty))
            .unwrap();
        Ok(())
    }

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
