use binaryninja::{
    platform::Platform,
    types::{CoreTypeParser, ParsedType, TypeParser, TypeParserResult},
};

pub(crate) const LK_TYPES_C_SRC_UBOOT: &'static str = r#"
/* LK image header */

union lk_hdr {
	struct {
		uint32_t magic;
		uint32_t size;
		char name[32];
		uint32_t loadaddr;
		uint32_t mode;
	};

	uint8_t data[512];
};

#define LK_PART_MAGIC		0x58881688
"#;

pub(crate) const LK_TYPES_LK_HEADER: &'static str = r#"

typedef struct ImageType {
    uint8_t _id;
    uint8_t reserved0;
    uint8_t reserved1;
    uint8_t _group;
} ImageType_t;

struct lk_hdr_32 {
    uint32_t magic;
    uint32_t _dsize;
    char _cname[0x20];
    uint32_t _maddr;
    uint32_t mode;
    uint32_t ext_magic;
    uint32_t hdr_size;
    uint32_t hdr_version;
    ImageType_t image_type;
    uint32_t image_list_end;
    uint32_t alignment;
    uint32_t _dsize_extend;
    uint32_t _maddr_extend;
    char _padding[0x1b0];
};
"#;

pub struct LkCPlatformTypes {
    parsed_types: TypeParserResult,
}

impl LkCPlatformTypes {
    pub fn new(platform_str: &str) -> Self {
        let platform = Platform::by_name(platform_str).unwrap();
        let plat_type_container = platform.type_container();
        let type_parser = CoreTypeParser::default();
        let parsed_types = type_parser
            .parse_types_from_source(
                LK_TYPES_LK_HEADER,
                "lk_types.h",
                &platform,
                &plat_type_container,
                &[],
                &[],
                "",
            )
            .unwrap();

        Self { parsed_types }
    }

    pub fn get_type_by_name(&self, name: &str) -> Option<ParsedType> {
        for t in &self.parsed_types.types {
            if t.name == name.into() {
                return Some(t.clone());
            }
        }
        None
    }

    pub fn get_all_types(&self) -> Option<Vec<ParsedType>> {
        if !self.parsed_types.types.is_empty() {
            return Some(self.parsed_types.types.clone());
        }
        None
    }
}
