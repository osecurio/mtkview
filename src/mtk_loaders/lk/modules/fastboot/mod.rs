use crate::mtk_loaders::lk::modules::fastboot::FuncFinder::FindCall;
use crate::mtk_loaders::lk::modules::fastboot::FuncFinder::FindRootFuncOfCall;
use crate::mtk_loaders::lk::modules::fastboot::types::FASTBOOT_INIT_32_STR;
use crate::mtk_loaders::lk::modules::fastboot::types::FASTBOOT_PUBLISH_STR_MDS;
use crate::mtk_loaders::lk::modules::fastboot::types::FASTBOOT_PUBLISH_STR_VERSION;
use crate::mtk_loaders::lk::modules::fastboot::types::FASTBOOT_REGISTER_STR;
use crate::mtk_loaders::lk::modules::fastboot::types::get_fastboot_types;
use crate::util::find_magic_all;
use binaryninja::binary_view::search::SearchQuery;
use binaryninja::rc::Ref;
use binaryninja::string::IntoCStr;
use binaryninja::symbol::Symbol;
use binaryninja::symbol::SymbolType;
use binaryninja::{
    binary_view::{BinaryView, BinaryViewExt, StringReference},
    function::Function,
    medium_level_il::MediumLevelILLiftedInstructionKind,
};
use tracing::warn;
use tracing::{debug, error, info};

mod types;

// The differences are so dynamic maybe each heuristic should be a trait impl???
// Or an analysis + type structure that loads the types and assigns them idk
#[derive(Debug)]
pub struct FastBootAnalysis {
    fb_init_sra: Option<u64>,
    fb_register_sra: Option<u64>,
    fb_publish_sra: Option<u64>,
}

impl FastBootAnalysis {
    pub fn get_sra_from_type_name(&self, type_name: &str) -> Option<u64> {
        match type_name {
            "fastboot_init" => self.fb_init_sra,
            "fastboot_register" => self.fb_register_sra,
            "fastboot_publish" => self.fb_publish_sra,
            _ => None,
        }
    }
}
/*
 * PARAM: LiftedCall { output: [], dest: MediumLevelILLiftedInstruction { address: 56020734, instr_index: MediumLevelInstructionIndex(9), expr_index: MediumLevelExpressionIndex(23), size: 4, kind: ConstPtr(Constant { constant: 5602016C }) }, params: [MediumLevelILLiftedInstruction { address: 56020732, instr_index: MediumLevelInstructionIndex(9), expr_index: MediumLevelExpressionIndex(1F), size: 4, kind: ConstPtr(Constant { constant: 560411EC }) }, MediumLevelILLiftedInstruction { address: 56020730, instr_index: MediumLevelInstructionIndex(9), expr_index: MediumLevelExpressionIndex(20), size: 4, kind: Const(Constant { constant: 56021169 }) }, MediumLevelILLiftedInstruction { address: 5602072A, instr_index: MediumLevelInstructionIndex(9), expr_index: MediumLevelExpressionIndex(21), size: 4, kind: Const(Constant { constant: 1 }) }, MediumLevelILLiftedInstruction { address: 5602072E, instr_index: MediumLevelInstructionIndex(9), expr_index: MediumLevelExpressionIndex(22), size: 4, kind: Const(Constant { constant: 0 }) }] }
 */

/*
 * TODO
 * Add mechanism to easily add strings with func ref definitions
 * Iterative approach to heuristic strings
 * Routine for getting root function of a string ref for example fastboot_init() inside the fastboot init routine
 * LK2 seems like it may be different or some stuff
 */
fn find_call_with_param(func: &Function, param_str_addr: u64, param_idx: usize) -> Option<u64> {
    for bb in func.medium_level_il().unwrap().basic_blocks().iter() {
        for ins in bb.iter() {
            match ins.lift().kind {
                MediumLevelILLiftedInstructionKind::Call(lc) => {
                    //debug!("PARAM: {:#X?}", lc.params);
                    let Some(first_param) = lc.params.get(param_idx) else {
                        continue;
                    };
                    if let MediumLevelILLiftedInstructionKind::ConstPtr(const_ptr) =
                        first_param.kind
                    {
                        let MediumLevelILLiftedInstructionKind::ConstPtr(func_ptr) = lc.dest.kind
                        else {
                            continue;
                        };
                        debug!(
                            "Target SR addr?? : 0x{:x} | String addr?? : 0x{:x} | Instruction addr?? : 0x{:x}",
                            func_ptr.constant, const_ptr.constant, lc.dest.address
                        );

                        if param_str_addr == const_ptr.constant {
                            return Some(func_ptr.constant);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

enum FuncFinder<'s> {
    FindCall(&'s str),
    FindRootFuncOfCall(&'s str),
}

impl<'s> FuncFinder<'s> {
    pub fn length(&self) -> usize {
        match self {
            Self::FindCall(s) => s.len(),
            Self::FindRootFuncOfCall(s) => s.len(),
        }
    }
}

impl<'s> AsRef<&'s str> for FuncFinder<'s> {
    fn as_ref(&self) -> &&'s str {
        match self {
            Self::FindCall(s) => s,
            Self::FindRootFuncOfCall(s) => s,
        }
    }
}

fn find_unlisted_binja_string(bv: &BinaryView, find: &str) -> Option<Vec<u64>> {
    // Fallback attempt
    println!("Heuristic string not found! Falling back to raw buffer read..");
    let lk_data_section_addr_range = bv.section_by_name("lk_data").unwrap().address_range();
    let heur_buf = bv.read_vec(
        lk_data_section_addr_range.start,
        lk_data_section_addr_range.end as usize,
    );

    find_magic_all(&heur_buf, find.as_bytes())
}

fn find_func_from_str_arg(bv: &BinaryView, find: FuncFinder, param_idx: usize) -> Option<Vec<u64>> {
    let str_refs = bv
        .strings()
        .iter()
        .filter_map(|s| {
            if s.length == find.length() {
                Some(s)
            } else {
                None
            }
        })
        .collect::<Vec<StringReference>>();
    if str_refs.is_empty() {
        return None;
    }
    debug!("Got {} strings!", str_refs.len());
    let found_str = match str_refs.iter().find_map(|s| {
        let Some(rawr_s) = bv.read_c_string_at(s.start, s.length) else {
            return None;
        };
        if rawr_s.to_str().unwrap() == *find.as_ref() {
            Some((s.start, s.length, rawr_s.clone()))
        } else {
            None
        }
    }) {
        Some(found_string_data) => found_string_data,
        None => {
            debug!("No fastboot strings found!");
            /*
            let Some(possible_strings) = find_unlisted_binja_string(bv, find.as_ref()) else {
                return None;
            };*/
            let mut string_search_result = None;
            info!(
                "Heuristic pattern '{:x?}' not found! Falling back to raw buffer read..",
                find.as_ref().as_bytes()
            );

            bv.search(
                &SearchQuery::new(*find.as_ref()).ignore_case(false),
                |sq_str_addr, sq_str_buf| {
                    debug!("Checking for code refs to address: 0x{:x}", sq_str_addr);
                    let code_ref_count = bv.code_refs_to_addr(sq_str_addr);
                    if code_ref_count.len() > 0 {
                        info!("Found possible string @ 0x{:x}", sq_str_addr);
                        let last_byte = find.as_ref().as_bytes().last().unwrap();
                        let s_cstr = if *last_byte == 0x00 {
                            bv.read_c_string_at(sq_str_addr, sq_str_buf.len() - 1)
                                .unwrap()
                        } else {
                            bv.read_c_string_at(sq_str_addr, sq_str_buf.len()).unwrap()
                        };

                        let s_cstr_len = s_cstr.to_str().unwrap().len();
                        string_search_result = Some((sq_str_addr, s_cstr_len, s_cstr));
                        return false;
                    }
                    return true;
                },
            );
            if let None = string_search_result {
                return None;
            }
            debug!(
                "Got fallback match @ 0x{:x}: '{}' (Bytes: {})",
                string_search_result.as_ref().unwrap().0,
                string_search_result.as_ref().unwrap().2.to_str().unwrap(),
                string_search_result.as_ref().unwrap().1
            );
            string_search_result.unwrap()
        }
    };
    debug!(
        "Got string: 0x{:X}, 0x{:X}, '{}'",
        found_str.0,
        found_str.1,
        found_str.2.to_str().unwrap()
    );

    let code_refs = bv.code_refs_to_addr(found_str.0);
    if code_refs.is_empty() {
        debug!("Got no code refs for {}", found_str.0);
        return None;
    };
    debug!("Code refs count : 0x{:X}", code_refs.len());

    match find {
        FuncFinder::FindCall(_find_str) => {
            let mut funcs = vec![];
            let func_iter = code_refs
                .iter()
                .map(|cr| cr.func.unwrap())
                .collect::<Vec<Ref<Function>>>();
            for f in func_iter {
                if let Some(func_call_addr) = find_call_with_param(&f, found_str.0, param_idx) {
                    funcs.push(func_call_addr);
                }
            }
            return Some(funcs);
        }
        FuncFinder::FindRootFuncOfCall(_find_str) => {
            let funcs = code_refs
                .iter()
                .map(|f| f.func.unwrap().lowest_address())
                .collect();
            Some(funcs)
        }
    }
}

pub(crate) fn detect_fb_subroutines(bv: &BinaryView) -> FastBootAnalysis {
    let fb_init_sra = match find_func_from_str_arg(bv, FindRootFuncOfCall(FASTBOOT_INIT_32_STR), 0)
    {
        Some(sra) => Some(sra[0]),
        None => None,
    };
    // if None check for 64 bit str
    let fb_register_sra = match find_func_from_str_arg(bv, FindCall(FASTBOOT_REGISTER_STR), 0) {
        Some(sra) => Some(sra[0]),
        None => None,
    };

    let fb_publish_sra_mds = find_func_from_str_arg(bv, FindCall(FASTBOOT_PUBLISH_STR_MDS), 0);
    let fb_publish_sra_version =
        find_func_from_str_arg(bv, FindCall(FASTBOOT_PUBLISH_STR_VERSION), 0);

    let mut fb_publish_sra = None;
    if fb_publish_sra_mds.is_some() && fb_publish_sra_version.is_some() {
        for addr in fb_publish_sra_mds.unwrap() {
            if let Some(pub_sra) = fb_publish_sra_version
                .as_ref()
                .unwrap()
                .iter()
                .map(|v_addr| v_addr.to_owned())
                .find(|v_addr| addr == *v_addr)
            {
                fb_publish_sra = Some(pub_sra);
                break;
            }
        }
    }

    FastBootAnalysis {
        fb_init_sra,
        fb_register_sra,
        fb_publish_sra,
    }
}

fn define_fastboot_routines(bv: &BinaryView, analysis: FastBootAnalysis) -> Result<(), ()> {
    for pt in get_fastboot_types(bv) {
        let Some(subroutine_addr) = analysis.get_sra_from_type_name(&pt.name.to_string().as_str())
        else {
            continue;
        };
        if let Some(f) = bv.functions().iter().find(|f| f.start() == subroutine_addr) {
            debug!("Found function: 0x{:x}", f.start());
            f.set_user_type(&*pt.ty);
            let fb_reg_sym = Symbol::builder(
                SymbolType::Function,
                &pt.name.to_string().as_str(),
                subroutine_addr,
            )
            .create();
            bv.define_user_symbol(&fb_reg_sym);
        }
    }
    Ok(())
}

pub(crate) fn populate_fastboot_srs(bv: &BinaryView) -> Result<(), ()> {
    let analysis = detect_fb_subroutines(bv);
    let res = define_fastboot_routines(bv, analysis);
    bv.update_analysis();
    res
}
