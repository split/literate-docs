use markdown::mdast::Node;

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub lang: Option<String>,
    pub value: String,
}

pub fn extract_code_blocks(node: &Node) -> Vec<CodeBlock> {
    fn walk(node: &Node, blocks: &mut Vec<CodeBlock>) {
        if let Some(children) = node.children() {
            for child in children.iter() {
                if let Node::Code(code) = child {
                    blocks.push(CodeBlock {
                        lang: code.lang.clone(),
                        value: code.value.clone(),
                    });
                } else {
                    walk(child, blocks);
                }
            }
        }
    }

    let mut blocks = Vec::new();
    walk(node, &mut blocks);
    blocks
}
