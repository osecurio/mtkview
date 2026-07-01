use std::{collections::HashMap, fmt, ops::Range};

use binaryninja::{section, segment::SegmentFlags};
use tracing::debug;

use crate::{
    BinaryViewResult,
    mtk_loaders::lk::lk_headers::{
        MTKLK_CODE_ENTRY_POINT_OFFSET, MTKLK_CODE_LOAD_ADDR_OFFSET, MtkLkHeader,
    },
    util::{find_magic_first, read_data_slice_u32, read_data_slice_u64},
};

pub(crate) mod lk_headers;
pub(crate) mod lk_types;
pub(crate) mod view;

#[derive(Debug, Clone)]
struct LkRomData {
    data: Vec<u8>,
    data_memory_load_address: Option<u64>,
    entrypoint: Option<u64>,
}

impl LkRomData {
    pub fn new(data_slice: &[u8], lk_code: bool) -> Self {
        let data = data_slice.to_vec();
        let mut data_memory_load_address = None;
        let mut entrypoint = None;

        if lk_code {
            if let Some(bits_64_base_addr) = get_aarch64_base_addr(&data) {
                data_memory_load_address = Some(bits_64_base_addr);
                entrypoint = Some(bits_64_base_addr);
            } else {
                data_memory_load_address =
                    Some(read_data_slice_u32(&data, MTKLK_CODE_LOAD_ADDR_OFFSET).unwrap() as u64);
                entrypoint =
                    Some(read_data_slice_u32(&data, MTKLK_CODE_ENTRY_POINT_OFFSET).unwrap() as u64);
            }
        }

        Self {
            data,
            data_memory_load_address,
            entrypoint,
        }
    }
}

fn get_aarch64_base_addr(data: &[u8]) -> Option<u64> {
    // Find the following integers in a row
    // 000004f0  int64_t data_4f0 = 0x60000000000400
    // 000004f8  int64_t data_4f8 = 0x60000000000404
    // 00000500  int64_t data_500 = 0x60000000000708
    // "\x00\x04\x00\x00\x00\x00\x60\x00\x04\x04\x00\x00\x00\x00\x60\x00\x08\x07\x00\x00\x00\x00\x60\x00"
    // The 64 bit integer after it should have a 0 mask of 0x5
    // 0xffff000050700000 & 0xfffff == 0x0
    //
    let Some(sig_offset) = find_magic_first(data, b"\x00\x04\x00\x00\x00\x00\x60\x00\x04\x04\x00\x00\x00\x00\x60\x00\x08\x07\x00\x00\x00\x00\x60\x00") else {
        return None;
    };
    read_data_slice_u64(data, sig_offset + (0x3 * size_of::<u64>()))
}

#[derive(Debug, Clone)]
pub(crate) struct LKRomSegmentized {
    header_mapped_addr_range: Range<u64>,
    header_file_backing: Range<u64>,
    header_mapped_seg_flags: SegmentFlags,
    data_mapped_addr_range: Range<u64>,
    data_file_backing: Range<u64>,
    data_mapped_seg_flags: SegmentFlags,
}

impl LKRomSegmentized {
    pub fn get_header_mapped_addr_range(&self) -> Range<u64> {
        self.header_mapped_addr_range.clone()
    }
    pub fn get_header_file_backing(&self) -> Range<u64> {
        self.header_file_backing.clone()
    }
    pub fn get_header_mapped_seg_flags(&self) -> SegmentFlags {
        self.header_mapped_seg_flags
    }
    pub fn get_data_mapped_addr_range(&self) -> Range<u64> {
        self.data_mapped_addr_range.clone()
    }
    pub fn get_data_file_backing(&self) -> Range<u64> {
        self.data_file_backing.clone()
    }
    pub fn get_data_mapped_seg_flags(&self) -> SegmentFlags {
        self.data_mapped_seg_flags
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LKRomSection {
    name_root: String,
    header: MtkLkHeader,
    data: LkRomData,
    file_offset: u64,
}

impl LKRomSection {
    pub fn full_size(&self) -> u64 {
        self.header.get_full_size()
    }

    pub fn get_section_start(&self) -> u64 {
        self.file_offset
    }
    pub fn get_section_end(&self) -> u64 {
        self.file_offset + self.full_size()
    }

    pub fn get_header_size(&self) -> u64 {
        self.header.header_size() as u64
    }

    pub fn is_lk(&self) -> bool {
        self.header.is_lk_code()
    }

    pub fn get_segmentized(&self) -> LKRomSegmentized {
        let header_mapped_addr_range = Range {
            start: *self.data.data_memory_load_address.as_ref().unwrap() as u64
                - self.get_header_size(),
            end: *self.data.data_memory_load_address.as_ref().unwrap() as u64,
        };
        let header_file_backing = Range {
            start: self.file_offset,
            end: self.get_header_size(),
        };
        let header_mapped_seg_flags = SegmentFlags::new()
            .contains_code(false)
            .contains_data(false)
            .writable(false)
            .readable(false);
        let data_mapped_addr_range = Range {
            start: *self.data.data_memory_load_address.as_ref().unwrap() as u64,
            end: *self.data.data_memory_load_address.as_ref().unwrap() as u64
                + self.header.data_size() as u64,
        };
        let data_file_backing = Range {
            start: self.file_offset + self.get_header_size(),
            end: self.file_offset + self.header.get_full_size(),
        };
        let data_mapped_seg_flags = SegmentFlags::new()
            .contains_code(true)
            .contains_data(true)
            .readable(true)
            .writable(true)
            .executable(true);
        LKRomSegmentized {
            header_mapped_addr_range,
            header_file_backing,
            header_mapped_seg_flags,
            data_mapped_addr_range,
            data_file_backing,
            data_mapped_seg_flags,
        }
    }
}

pub(crate) struct MTKLkLoader {
    lk_sections: HashMap<String, LKRomSection>,
}

impl fmt::Display for MTKLkLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::new();
        for seg in &self.lk_sections {
            s.push_str(format!("{}\n", seg.0).as_str());
        }
        write!(f, "{}", s)
    }
}

impl MTKLkLoader {
    pub fn new(data: &[u8]) -> BinaryViewResult<Self> {
        let data_full_len = data.len();
        let mut data_curr_i = 0;

        let mut lk_sections = HashMap::<String, LKRomSection>::new();

        loop {
            let Some(loaded_lk_header) = MtkLkHeader::load(&data[data_curr_i..], false) else {
                debug!("No MTK LK Header found.. breaking.");
                break;
            };
            let name_root = loaded_lk_header.get_name_root();
            let full_size = loaded_lk_header.get_full_size();
            let lk_code = loaded_lk_header.is_lk_code();
            let lk_header_size = loaded_lk_header.header_size() as usize;
            let loaded_lk_data = LkRomData::new(&data[data_curr_i + lk_header_size..], lk_code);
            //debug!("Loaded LK Data: {:#X?}", loaded_lk_data);

            let loaded_lk_segment = LKRomSection {
                name_root: loaded_lk_header.get_name_root(),
                header: loaded_lk_header,
                data: loaded_lk_data,
                file_offset: data_curr_i as u64,
            };

            lk_sections.insert(name_root, loaded_lk_segment);
            if (full_size as usize + data_curr_i) as usize == data_full_len {
                // Loaded all LKs
                debug!(
                    "Loaded all LKs: {} == data_full_len",
                    (full_size as usize + data_curr_i)
                );
                break;
            } else {
                data_curr_i += full_size as usize;
            }
        }

        if data_curr_i == 0 {
            // Found no LK magic
            debug!("Load failure: data_curr_i == 0");
            return Err(());
        }

        debug!("Got {} LK sections", lk_sections.len());
        Ok(Self { lk_sections })
    }

    pub fn get_sections(&self) -> HashMap<String, LKRomSection> {
        self.lk_sections.clone()
    }

    pub fn get_address_size(&self) -> usize {
        if self.lk_sections.keys().any(|s| s.starts_with("bl2_ext")) {
            8
        } else {
            4
        }
    }
    pub fn get_entry_point(&self, section_name: &str) -> u64 {
        let s = self.lk_sections.get(section_name).unwrap();
        s.data.entrypoint.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mtk_lk_loader() {
        let f = include_bytes!("../../../testbins/lk.img");
        let _ = MTKLkLoader::new(f).unwrap();
    }
}

/*
pub(crate) struct LkMd1RomHookContext {
    entry_point: u32,
    data_base_address: u32,
    done: bool,
}

impl Default for LkMd1RomHookContext {
    fn default() -> Self {
        Self {
            entry_point: 0,
            data_base_address: 0,
            done: false,
        }
    }
}

impl LkMd1RomHookContext {
    pub fn new(entry_point: u32, data_base_address: u32) -> Self {
        Self {
            entry_point,
            data_base_address,
            done: false,
        }
    }
}

impl BinaryViewEventHandler for LkMd1RomHookContext {
    fn on_event(&self, binary_view: &binaryninja::binary_view::BinaryView) {
        if self.done {
            return;
        }
        let Some(pv) = binary_view.parent_view() else {
            debug!("waiting for parent view..");
            return;
        };
        let Some(lk_code) = pv.section_by_name("lk_data") else {
            debug!("MD1ROM Has no LK.");
            return;
        };
        let lk_code_address_range = lk_code.address_range();
        if lk_code.len() == 0 {
            return;
        }

        println!(
            "lk_data: {:#X} - {:#X}",
            lk_code_address_range.start, lk_code_address_range.end
        );

        let lk_code_data = {
            let b = pv
                .read_buffer(lk_code_address_range.start, lk_code.len())
                .unwrap();
            b.get_data().to_owned()
        };
        println!("SIZE: {}", lk_code_data.len());
        let code_base_addr = read_data_slice_u32(&lk_code_data, 0x74).unwrap();
        info!("Got load base addr: {:#X}", code_base_addr);

        let new_sec_range = Range {
            start: code_base_addr as u64,
            end: lk_code_data.len() as u64 + code_base_addr as u64,
        };

        let section_builder = SectionBuilder::new("LK Code".to_string(), new_sec_range);
        pv.remove_auto_section(lk_code.name());
        pv.add_section(section_builder);
        //binary_view.add_section();
    }
}
*/
