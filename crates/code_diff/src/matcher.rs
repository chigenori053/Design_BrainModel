use code_ir::program_v1::{Function, FunctionInput, Module};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionMatch<'a> {
    pub old: &'a Function,
    pub new: &'a Function,
}

pub fn match_functions<'a>(old: &'a Module, new: &'a Module) -> Vec<FunctionMatch<'a>> {
    let mut matches = old
        .functions
        .iter()
        .filter_map(|old_fn| {
            new.functions
                .iter()
                .find(|new_fn| signatures_match(old_fn, new_fn))
                .map(|new_fn| FunctionMatch {
                    old: old_fn,
                    new: new_fn,
                })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|lhs, rhs| lhs.old.name.cmp(&rhs.old.name));
    matches
}

pub fn signatures_match(lhs: &Function, rhs: &Function) -> bool {
    lhs.name == rhs.name
        || (lhs.inputs == rhs.inputs
            && lhs.outputs == rhs.outputs
            && same_parameter_names(&lhs.inputs, &rhs.inputs))
}

fn same_parameter_names(lhs: &[FunctionInput], rhs: &[FunctionInput]) -> bool {
    lhs.iter()
        .map(|input| &input.name)
        .eq(rhs.iter().map(|input| &input.name))
}
