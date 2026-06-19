use crate::util::{find_magic_first, read_data_slice_n, read_data_slice_u32};

pub const MTKLK_MAGIC: &'static [u8; 0x4] = b"\x88\x16\x88\x58";
pub const MTKLK_MAGIC_INT: u32 = 0x58881688;
pub const MTKLK_MAGIC2: &'static [u8; 0x4] = b"\x89\x16\x89\x58";
pub const MTKLK_MAGIC2_INT: u32 = 0x58891689;
pub const MTKLK_HEADER_DEFAULT_LEN: usize = 0x200;
/*
 * 0x74 bytes into an lk data image is the base address of lk code&data
 *
 */

pub struct MdHeaderExt {
    magic2: u32,
    offset: u32,
}

pub(crate) struct MtkLkHeader {
    magic: u32,
    size: u32,
    name: [u8; 0x20],
    loadaddr: u32,
    mode: u32,
    // Optional rest of struct
    md1_ext: Option<MdHeaderExt>,
}

impl MtkLkHeader {
    pub fn load(data: &[u8]) -> Option<Self> {
        let mut offset = 0;

        // Magic search
        let Some(magic_offset) = find_magic_first(data, MTKLK_MAGIC) else {
            return None;
        };
        offset = magic_offset;

        let magic = {
            let m = read_data_slice_u32(data, offset).unwrap();
            if m != MTKLK_MAGIC2_INT {
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
        offset += size_of::<u32>();
        let loadaddr = read_data_slice_u32(data, offset).unwrap();
        offset += size_of::<u32>();
        let mode = read_data_slice_u32(data, offset).unwrap();
        offset += size_of::<u32>();
        let md1_ext = {
            let m2 = read_data_slice_u32(data, offset).unwrap();
            if m2 != MTKLK_MAGIC2_INT {
                return None;
            }
            let data_offset = read_data_slice_u32(data, offset).unwrap();

            Some(MdHeaderExt {
                magic2: m2,
                offset: data_offset,
            })
        };

        Some(Self {
            magic,
            size,
            name,
            loadaddr,
            mode,
            md1_ext,
        })
    }

    pub fn get_full_size(&self) -> u64 {
        self.size as u64
    }

    pub fn is_md_ext(&self) -> bool {
        self.md1_ext.is_some()
    }

    pub fn get_name_root(&self) -> String {
        if let Ok(s) = str::from_utf8(&self.name) {
            return s.to_string();
        }
        let mut hs = String::from("seg_");
        for b in self.name {
            hs.push_str(format!("{:02x}", b).as_str());
        }
        hs
    }
}
