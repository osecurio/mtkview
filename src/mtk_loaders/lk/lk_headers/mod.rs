use tracing::{debug, info, warn};

use crate::util::{find_magic_first, read_data_slice_n, read_data_slice_u32};

pub const MTKLK_MAGIC: &'static [u8; 0x4] = b"\x88\x16\x88\x58";
pub const MTKLK_MAGIC_INT: u32 = 0x58881688;
pub const MTKLK_MAGIC2: &'static [u8; 0x4] = b"\x89\x16\x89\x58";
pub const MTKLK_MAGIC2_INT: u32 = 0x58891689;
pub const MTKLK_HEADER_DEFAULT_LEN: usize = 0x200;
pub const MTKLK_CODE_LOAD_ADDR_OFFSET: usize = 0x74;
pub const MTKLK_CODE_LOAD_ADDR_END_OFFSET: usize = 0x78;
pub const MTKLK_CODE_ENTRY_POINT_OFFSET: usize = 0x7c;
/*
 * 0x74 bytes into an lk data image is the base address of lk code&data
 *
 */

#[derive(Debug, Clone)]
pub struct MdHeaderExt {
    magic2: u32,
    offset: u32,
    hdr_version: u32,
    image_type: u32,
    image_list_end: u32,
    alignment: u32,
    _dsize_extend: u32,
    _maddr_extend: u32,
    _padding: [u8; 0x1b0],
}

#[derive(Debug, Clone)]
pub(crate) struct MtkLkHeader {
    magic: u32,
    size: u32,
    name: [u8; 0x20],
    loadaddr: u32,
    mode: u32,
    // Optional rest of struct
    md1_ext: Option<MdHeaderExt>,
    lk_code: bool,
}

impl MtkLkHeader {
    pub fn load(data: &[u8], force_lk: bool) -> Option<Self> {
        // Magic search
        let Some(magic_offset) = find_magic_first(data, MTKLK_MAGIC) else {
            warn!("No LK Magic found!");
            return None;
        };
        info!("Magic Found @ 0x{:X}", magic_offset);
        let mut offset = magic_offset;

        let magic = {
            let m = read_data_slice_u32(data, offset).unwrap();
            if m != MTKLK_MAGIC_INT {
                return None;
            }
            m
        };
        offset += size_of::<u32>();
        let size = read_data_slice_u32(data, offset).unwrap();
        offset += size_of::<u32>();
        let name = *read_data_slice_n(data, offset, 0x20)
            .unwrap()
            .as_array()
            .unwrap();

        let lk_code = if !force_lk {
            if let Ok(s) = str::from_utf8(&name) {
                debug!("Got MD name header: {}", s);
                let tmp_s = s.trim_end_matches('\0');
                match tmp_s.starts_with("lk") && tmp_s.len() == 2 {
                    true => {
                        debug!("Setting lk_code to 'true'");
                        true
                    }
                    false => {
                        debug!("Setting lk_code to 'false'");
                        false
                    }
                }
            } else {
                debug!("str parse failure - Setting lk_code to 'false'");
                false
            }
        } else {
            force_lk
        };

        debug!("lk_code = {}", lk_code);

        offset += 0x20;
        let loadaddr = read_data_slice_u32(data, offset).unwrap();
        offset += size_of::<u32>();
        let mode = read_data_slice_u32(data, offset).unwrap();
        offset += size_of::<u32>();

        let m2 = read_data_slice_u32(data, offset).unwrap();
        if m2 != MTKLK_MAGIC2_INT {
            info!("Magic2 NOT Found @ 0x{:X}", offset);
            return None;
        }

        info!("Magic2 Found @ 0x{:X}", offset);
        offset += size_of::<u32>();
        let data_offset = read_data_slice_u32(data, offset).unwrap();

        offset += size_of::<u32>();
        let hdr_version = read_data_slice_u32(data, offset).unwrap();

        offset += size_of::<u32>();
        let image_type = read_data_slice_u32(data, offset).unwrap();

        offset += size_of::<u32>();
        let image_list_end = read_data_slice_u32(data, offset).unwrap();

        offset += size_of::<u32>();
        let alignment = read_data_slice_u32(data, offset).unwrap();

        offset += size_of::<u32>();
        let _dsize_extend = read_data_slice_u32(data, offset).unwrap();

        offset += size_of::<u32>();
        let _maddr_extend = read_data_slice_u32(data, offset).unwrap();

        offset += size_of::<u32>();
        let _padding = *read_data_slice_n(data, offset, 0x1b0)
            .unwrap()
            .as_array()
            .unwrap();

        let md1_ext = Some(MdHeaderExt {
            magic2: m2,
            offset: data_offset,
            hdr_version,
            image_type,
            image_list_end,
            alignment,
            _dsize_extend,
            _maddr_extend,
            _padding,
        });

        Some(Self {
            magic,
            size,
            name,
            loadaddr,
            mode,
            md1_ext,
            lk_code,
        })
    }

    pub fn get_full_size(&self) -> u64 {
        self.size as u64 + self.header_size() as u64
    }

    pub fn is_lk_code(&self) -> bool {
        self.lk_code
    }

    pub fn data_size(&self) -> u32 {
        self.size
    }

    pub fn header_size(&self) -> u32 {
        self.md1_ext.as_ref().unwrap().offset
    }

    pub fn get_name_root(&self) -> String {
        if let Ok(s) = String::from_utf8(self.name.to_vec()) {
            debug!("String: {} has length: {}", s, s.len());
            return s.trim_end_matches('\0').to_string();
        }
        let mut hs = String::from("seg_");
        for b in self.name {
            hs.push_str(format!("{:02x}", b).trim_end_matches('\0'));
        }
        hs
    }
}
