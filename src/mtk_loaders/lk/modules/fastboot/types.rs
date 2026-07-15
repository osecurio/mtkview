use binaryninja::{
    binary_view::{BinaryView, BinaryViewExt},
    platform::Platform,
    types::{CoreTypeParser, ParsedType, QualifiedNameAndType, TypeParser},
};
use tracing::debug;

pub(crate) const LK_TYPES_FASTBOOT_COMMON: &'static str = r#"
int fastboot_init(void *base, unsigned int size);
void fastboot_register(const char *prefix, void (*handle)(const char *arg, void *data, unsigned int sz), int allowed_when_security_on, int forbidden_when_lock_on);
void fastboot_publish(const char *name, const char *value);
"#;

pub(crate) const FASTBOOT_INIT_32_STR: &str = "fastboot_init()\n";
pub(crate) const FASTBOOT_REGISTER_STR: &str = "getvar:";
pub(crate) const FASTBOOT_PUBLISH_STR_MDS: &str = "max-download-size";
pub(crate) const FASTBOOT_PUBLISH_STR_VERSION: &str = "version\x00";

pub fn get_fastboot_types(bv: &BinaryView) -> Vec<QualifiedNameAndType> {
    let mut parsed_types = vec![];
    for func_proto in LK_TYPES_FASTBOOT_COMMON.lines() {
        if func_proto.is_empty() {
            continue;
        }
        let platform = Platform::by_name(&bv.default_platform().unwrap().name()).unwrap();
        let plat_type_container = platform.type_container();
        let type_parser = CoreTypeParser::default();
        println!(
            "Attempting type: '{}', Platform: {}",
            func_proto,
            platform.name()
        );
        let parsed_type = type_parser
            .parse_type_string(func_proto, &platform, &plat_type_container)
            .unwrap();
        parsed_types.push(parsed_type);
    }
    debug!("Loaded {} fastboot types!", parsed_types.len());
    parsed_types
}
