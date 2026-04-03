use markdown::mdast::Node;
use crate::execute_code_blocks::ExecutableCodeBlock;

pub fn extract_executable_code_blocks(node: &Node) -> Vec<ExecutableCodeBlock> {
    match node.children() {
        Some(children) => children
            .iter()
            .flat_map(|child| {
                if let Node::Code(code) = child {
                    ExecutableCodeBlock::try_from(code).ok().into_iter().collect()
                } else {
                    extract_executable_code_blocks(child)
                }
            })
            .collect(),
        None => Vec::new(),
    }
}
