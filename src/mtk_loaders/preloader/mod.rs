use std::{collections::HashMap, fmt, ops::Range};

use binaryninja::{data_buffer::DataBuffer, segment::SegmentFlags};
use tracing::{debug, error};

use crate::{
    mtk_loaders::preloader::gfh_headers::{GfhHeader, MtkGfhHeader},
    util::find_magic_first,
};

pub(crate) mod gfh_headers;
pub(crate) mod view;

pub const MTKPL_MAGIC: &'static [u8; 8] = b"\x4d\x4d\x4d\x01\x38\x00\x00\x00";
pub const MTKPL_LOAD_ADDRESS_OFFSET: u64 = 0x1C;
pub const MTKPL_ENTRY_POINT_OFFSET: u64 = 0x28;
pub const MTKPL_BIN_SIZE_OFFSET: u64 = 0x20;
pub const MTKPL_ADDRESS_WIDTH: u64 = 0x4;

#[derive(Clone)]
pub struct SegmentMappingData {
    pub mapped_addr_range: Range<u64>,
    pub mapped_segment_flags: SegmentFlags,
    pub file_backing: Range<u64>,
}

impl SegmentMappingData {
    fn new(
        mapped_addr_range: Range<u64>,
        file_backing: Range<u64>,
        mapped_segment_flags: SegmentFlags,
    ) -> Self {
        Self {
            mapped_addr_range,
            file_backing,
            mapped_segment_flags,
        }
    }
}

#[derive(Clone)]
pub struct SectionData {
    pub name: String,
    pub mapped_addr_range: Range<u64>,
}

impl SectionData {
    fn new(name: &str, mapped_addr_range: Range<u64>) -> Self {
        Self {
            name: name.to_string(),
            mapped_addr_range,
        }
    }
}

struct GeneralEMIInformation {
    mtk_bloader_string: String,
    emi_file_offset: usize,
    emi_buffer: Vec<u8>,
    signature_buffer: Vec<u8>,
    signature_file_offset: usize,
}

impl GeneralEMIInformation {
    fn new(data: &Vec<u8>, signature_length: usize, size: usize) -> Self {
        let mut emi_info = Self {
            mtk_bloader_string: String::new(),
            emi_file_offset: 0,
            emi_buffer: vec![],
            signature_buffer: vec![],
            signature_file_offset: 0,
        };

        Self::parse_bloader_emi(&mut emi_info, data, signature_length, size);

        emi_info
    }

    fn parse_bloader_emi(&mut self, data: &Vec<u8>, signature_length: usize, size: usize) {
        println!(
            "data size: 0x{:X}, emi_end_offset: 0x{:X}, size: 0x{:X}",
            data.len(),
            signature_length,
            size
        );
        //println!("{:?}", &data[..0x10]);
        let emi_end_offset_snipped = &data[..size - signature_length];
        //println!("{:X?}", &emi_end_offset_snipped);
        let emi_data_size = u32::from_le_bytes(
            *emi_end_offset_snipped
                [emi_end_offset_snipped.len() - size_of::<u32>()..emi_end_offset_snipped.len()]
                .as_array()
                .unwrap(),
        );
        println!("EMI Data Size: 0x{:X}", emi_data_size);
        println!(
            "Data SZ: 0x{:X}, EMI Data Offset From End: 0x{:X}, EMI Data SZ: 0x{:X}",
            data.len(),
            signature_length,
            emi_data_size
        );

        let signature_buffer = &data[size - signature_length..];
        let emi_data = &data[size - signature_length - emi_data_size as usize - 4
            ..size - signature_length as usize - 4];

        self.mtk_bloader_string = String::from_utf8(emi_data[..0x18].to_vec()).unwrap();
        self.emi_buffer = emi_data.to_vec();
        self.emi_file_offset = size - signature_length - emi_data_size as usize - 4;
        self.signature_file_offset = size - signature_length;
        self.signature_buffer = signature_buffer.to_vec();
        //println!("{:X?}", emi_data);
    }

    pub fn get_emi_data_blob(&self) -> Vec<u8> {
        self.emi_buffer.clone()
    }

    pub fn get_bloader_str(&self) -> String {
        self.mtk_bloader_string.clone()
    }

    pub fn get_emi_file_offset(&self) -> usize {
        self.emi_file_offset
    }
}

pub struct MTKPreloaderLoader {
    image_data: Vec<u8>,
    drained_data: Vec<u8>,
    segment_data: HashMap<String, SegmentMappingData>,
    section_data: HashMap<String, SectionData>,
    gfh_header_info: GfhHeader,
}

impl MTKPreloaderLoader {
    pub fn new(data: DataBuffer) -> Result<Self, ()> {
        let mut image_data = data.get_data().to_vec();
        let offset = if let Some(offset) = find_magic_first(&image_data, MTKPL_MAGIC) {
            offset
        } else {
            0
        };
        let drained_data = image_data.drain(0..offset).as_slice().to_vec();
        debug!(
            "Drained {} bytes.. First 4 bytes are now: {:?}",
            drained_data.len(),
            &image_data.as_slice()[0..4]
        );

        let Some(gfh_header_info) = GfhHeader::load(&image_data, 0) else {
            error!("Unable to load any GFH header.");
            return Err(());
        };

        let mut parser = Self {
            image_data,
            drained_data,
            segment_data: HashMap::new(),
            section_data: HashMap::new(),
            gfh_header_info,
        };

        let file_offset_to_gfh_header = Self::get_file_backed_start_offset(&parser);

        let Some(file_info) = parser.gfh_header_info.get_gfh_file_info() else {
            return Err(());
        };
        let load_addr = file_info.get_load_addr() as u64;
        let entry_offset = file_info.get_jump_offset() as u64;
        let entry_addr = entry_offset + load_addr;
        let hdr_full_size = file_info.get_hdr_full_size() as u64;
        let preloader_size = file_info.get_total_size() as usize;
        let header_size = file_info.get_hdr_size() as u64;
        debug!("header_size: {header_size:X} = {entry_addr:X} - {load_addr:X}");

        let signature_length = file_info.get_signature_size();

        let emi_parser = GeneralEMIInformation::new(
            &parser.image_data,
            signature_length as usize,
            preloader_size,
        );

        let code_data_map_start = hdr_full_size + load_addr;
        let code_data_map_end = code_data_map_start
            + (preloader_size as u64
                - hdr_full_size
                - signature_length as u64
                - emi_parser.emi_buffer.len() as u64
                - 0x4);
        let code_data_fb_start = file_offset_to_gfh_header as u64 + hdr_full_size;
        let code_data_fb_end = code_data_fb_start
            + (preloader_size as u64
                - entry_offset as u64
                - signature_length as u64
                - emi_parser.emi_buffer.len() as u64)
            - 0x4;
        println!(
            "code_data_map_start: 0x{code_data_map_start:X}, code_data_map_end: 0x{code_data_map_end:X}, code_data_fb_start: 0x{code_data_fb_start:X}, code_data_fb_end: 0x{code_data_fb_end:X}"
        );

        let emi_buffer_len = emi_parser.emi_buffer.len() as u64;
        let emi_map_start = load_addr + emi_parser.emi_file_offset as u64;
        let emi_map_end = emi_map_start + emi_buffer_len;
        let emi_fb_start = emi_parser.emi_file_offset as u64;
        let emi_fb_end = emi_parser.emi_file_offset as u64 + emi_buffer_len;

        let signature_map_start = load_addr + emi_parser.signature_file_offset as u64;
        let signature_map_end = signature_map_start + emi_parser.signature_buffer.len() as u64;
        let signature_fb_start = emi_parser.signature_file_offset as u64;
        let signature_fb_end =
            emi_parser.signature_file_offset as u64 + emi_parser.signature_buffer.len() as u64;

        let header_seg_flags = SegmentFlags::new()
            .readable(true)
            .contains_code(false)
            .contains_data(false)
            .deny_write(false)
            .executable(true);

        let header_segment = SegmentMappingData::new(
            Range {
                start: load_addr,
                end: load_addr + header_size,
            },
            Range {
                start: file_offset_to_gfh_header as u64,
                end: file_offset_to_gfh_header as u64 + header_size,
            },
            header_seg_flags,
        );

        let header_section = SectionData::new(
            ".gfh",
            Range {
                start: load_addr,
                end: load_addr + header_size as u64,
            },
        );

        // Segment Flags
        let code_data_seg_flags = SegmentFlags::new()
            .readable(true)
            .contains_code(true)
            .contains_data(true)
            .deny_write(false)
            .executable(true)
            .writable(true);

        // Segment Mapping Data
        let code_data_segment = SegmentMappingData::new(
            Range {
                start: code_data_map_start,
                end: code_data_map_end,
            },
            Range {
                start: code_data_fb_start,
                end: code_data_fb_end,
            },
            code_data_seg_flags,
        );

        println!("right before section code_data_map_end: {code_data_map_end:X}");
        let code_data_section = SectionData::new(
            ".code.data",
            Range {
                start: code_data_map_start,
                end: code_data_map_end,
            },
        );

        // Segment Flags
        let emi_data_seg_flags = SegmentFlags::new()
            .readable(true)
            .contains_code(false)
            .contains_data(true)
            .deny_write(false)
            .executable(false)
            .writable(false);

        let emi_data_segment = SegmentMappingData::new(
            Range {
                start: emi_map_start,
                end: emi_map_end,
            },
            Range {
                start: emi_fb_start,
                end: emi_fb_end,
            },
            emi_data_seg_flags,
        );

        let emi_data_section = SectionData::new(
            ".emi.data",
            Range {
                start: emi_map_start,
                end: emi_map_end,
            },
        );

        // Segment Flags
        let signature_data_seg_flags = SegmentFlags::new()
            .readable(true)
            .contains_code(false)
            .contains_data(true)
            .deny_write(false)
            .executable(false)
            .writable(false);

        let signature_data_segment = SegmentMappingData::new(
            Range {
                start: signature_map_start,
                end: signature_map_end,
            },
            Range {
                start: signature_fb_start,
                end: signature_fb_end,
            },
            signature_data_seg_flags,
        );

        let signature_data_section = SectionData::new(
            ".signature.data",
            Range {
                start: signature_map_start,
                end: signature_map_end,
            },
        );

        parser
            .segment_data
            .insert(".gfh".to_string(), header_segment);
        parser
            .segment_data
            .insert(".code.data".to_string(), code_data_segment);
        parser
            .section_data
            .insert(".gfh".to_string(), header_section);
        parser
            .section_data
            .insert(".code.data".to_string(), code_data_section);
        parser
            .segment_data
            .insert(".emi.data".to_string(), emi_data_segment);
        parser
            .section_data
            .insert(".emi.data".to_string(), emi_data_section);
        parser
            .segment_data
            .insert(".signature.data".to_string(), signature_data_segment);
        parser
            .section_data
            .insert(".signature.data".to_string(), signature_data_section);

        Ok(parser)
    }

    pub fn get_image_load_addr(&self) -> u32 {
        let load_addr = self.image_data[MTKPL_LOAD_ADDRESS_OFFSET as usize
            ..(MTKPL_LOAD_ADDRESS_OFFSET + MTKPL_ADDRESS_WIDTH) as usize]
            .to_owned();
        let la = load_addr
            .try_into()
            .unwrap_or_else(|_| [0; MTKPL_ADDRESS_WIDTH as usize]);
        u32::from_le_bytes(la)
    }

    pub fn get_entry_point_offset(&self) -> u32 {
        let entry_point = self.image_data[MTKPL_ENTRY_POINT_OFFSET as usize
            ..(MTKPL_ENTRY_POINT_OFFSET + MTKPL_ADDRESS_WIDTH) as usize]
            .to_owned();
        let ep = entry_point
            .try_into()
            .unwrap_or_else(|_| [0; MTKPL_ADDRESS_WIDTH as usize]);
        u32::from_le_bytes(ep)
    }

    pub fn get_entry_point(&self) -> u64 {
        return (self.get_image_load_addr() + self.get_entry_point_offset()) as u64;
    }

    pub fn get_type_addr(&self, type_name: &str) -> Option<u32> {
        self.gfh_header_info.get_gfh_header_addr_by_name(type_name)
    }

    pub fn get_preloader_size(&self) -> u32 {
        let size = self.image_data[MTKPL_BIN_SIZE_OFFSET as usize
            ..(MTKPL_BIN_SIZE_OFFSET + MTKPL_ADDRESS_WIDTH) as usize]
            .to_owned();
        let sz = size
            .try_into()
            .unwrap_or_else(|_| [0; MTKPL_ADDRESS_WIDTH as usize]);
        u32::from_le_bytes(sz)
    }

    pub fn get_signature_length(&self, ep_offset: usize) -> u32 {
        let signature_length = &self.image_data
            [ep_offset as usize + 4..ep_offset as usize + 4 + MTKPL_ADDRESS_WIDTH as usize];
        u32::from_le_bytes(*signature_length.as_array().unwrap())
    }

    /*
    pub fn get_emi_file_offset(&self) -> usize {
        self.image_data.len() - self.get_signature_length() as usize
    }
    */

    pub fn get_file_backed_start_offset(&self) -> usize {
        self.drained_data.len()
    }

    pub fn get_segments(&self) -> HashMap<String, SegmentMappingData> {
        self.segment_data.clone()
    }

    pub fn get_sections(&self) -> HashMap<String, SectionData> {
        self.section_data.clone()
    }
}

impl fmt::Display for MTKPreloaderLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = format!("Loaded Preloader Data:\n");
        if let Some(hs) = self.segment_data.get(".gfh") {
            s = format!(
                "{s}m .gfh -> 0x{:X} - 0x{:X}\n",
                hs.mapped_addr_range.start, hs.mapped_addr_range.end
            );
            s = format!(
                "{s}f .gfh -> 0x{:X} - 0x{:X}\n",
                hs.file_backing.start, hs.file_backing.end
            );
        };

        if let Some(cds) = self.segment_data.get(".code.data") {
            s = format!(
                "{s}m .code.data -> 0x{:X} - 0x{:X}\n",
                cds.mapped_addr_range.start, cds.mapped_addr_range.end
            );
            s = format!(
                "{s}f .code.data -> 0x{:X} - 0x{:X}\n",
                cds.file_backing.start, cds.file_backing.end
            );
        }
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use crate::mtk_loaders::preloader::GeneralEMIInformation;

    use super::{
        MTKPL_ADDRESS_WIDTH, MTKPL_BIN_SIZE_OFFSET, MTKPL_ENTRY_POINT_OFFSET,
        MTKPL_LOAD_ADDRESS_OFFSET, MTKPL_MAGIC,
    };
    use std::fs;

    macro_rules! header_formatter {
        () => {
            r#"
- magic:          0x{:016X?}
- load_addr:      0x{:08X?}
- size:           0x{:08X?}
- entry_offset:   0x{:08X?}
- signature_length:   0x{:08X?}
"#
        };
    }

    fn print_header_data(data: &Vec<u8>) {
        let magic = &data[..MTKPL_MAGIC.len()];
        let magic = u64::from_le_bytes(*magic.as_array().unwrap());
        let load_addr = &data[MTKPL_LOAD_ADDRESS_OFFSET as usize
            ..(MTKPL_LOAD_ADDRESS_OFFSET + MTKPL_ADDRESS_WIDTH) as usize];
        let load_addr = u32::from_le_bytes(*load_addr.as_array().unwrap());
        let size = &data[MTKPL_BIN_SIZE_OFFSET as usize
            ..MTKPL_BIN_SIZE_OFFSET as usize + MTKPL_ADDRESS_WIDTH as usize];
        let size = u32::from_le_bytes(*size.as_array().unwrap());
        let entry_offset = &data[MTKPL_ENTRY_POINT_OFFSET as usize
            ..MTKPL_ENTRY_POINT_OFFSET as usize + MTKPL_ADDRESS_WIDTH as usize];
        let entry_offset = u32::from_le_bytes(*entry_offset.as_array().unwrap());
        let signature_length = &data[MTKPL_ENTRY_POINT_OFFSET as usize + 4
            ..MTKPL_ENTRY_POINT_OFFSET as usize + 4 + MTKPL_ADDRESS_WIDTH as usize];
        let signature_length = u32::from_le_bytes(*signature_length.as_array().unwrap());

        println!(
            header_formatter!(),
            magic, load_addr, size, entry_offset, signature_length
        );
    }

    #[test]
    fn test_emi_parser() {
        let mut data = fs::read("./testbins/test.bin").unwrap();
        print_header_data(&data);
        let size = &data[MTKPL_BIN_SIZE_OFFSET as usize
            ..MTKPL_BIN_SIZE_OFFSET as usize + MTKPL_ADDRESS_WIDTH as usize];
        let size = u32::from_le_bytes(*size.as_array().unwrap());
        let signature_length = &data[MTKPL_ENTRY_POINT_OFFSET as usize + 4
            ..MTKPL_ENTRY_POINT_OFFSET as usize + 4 + MTKPL_ADDRESS_WIDTH as usize];
        let signature_length = u32::from_le_bytes(*signature_length.as_array().unwrap());
        let emi_info =
            GeneralEMIInformation::new(&mut data, signature_length as usize, size as usize);
        println!(
            "MTK BLOADER STRING: {}\nEMI Buffer Head: {:X?}",
            emi_info.mtk_bloader_string,
            &emi_info.emi_buffer[0..8]
        );
    }
}
