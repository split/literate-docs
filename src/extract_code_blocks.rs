use crate::execute_code_blocks::ExecutableCodeBlock;
use markdown::mdast::Node;

pub fn extract_executable_code_blocks(node: &Node) -> Vec<ExecutableCodeBlock> {
    match node.children() {
        Some(children) => children
            .iter()
            .flat_map(|child| {
                ExecutableCodeBlock::try_from(child)
                    .ok()
                    .into_iter()
                    .chain(extract_executable_code_blocks(child))
            })
            .collect(),
        None => Vec::new(),
    }
}
