use crate::output_node::{is_output_node, update_output_value, OutputInfo};
use markdown::mdast::Node;

pub fn fill_output_blocks(info: OutputInfo, outputs: &mut impl Iterator<Item = String>) -> Node {
    fn fill_children(children: &[Node], outputs: &mut impl Iterator<Item = String>) -> Vec<Node> {
        children
            .iter()
            .map(|child| fill_output_blocks_node(child, outputs))
            .collect()
    }

    fn fill_output_blocks_node(node: &Node, outputs: &mut impl Iterator<Item = String>) -> Node {
        if let Some(children) = node.children() {
            let filled = fill_children(children, outputs);
            let mut owned = node.to_owned();
            if let Some(children_mut) = owned.children_mut() {
                *children_mut = filled;
            }
            owned
        } else if is_output_node(node) {
            if let Some(output) = outputs.next() {
                let mut filled = node.to_owned();
                update_output_value(&mut filled, &output);
                filled
            } else {
                node.to_owned()
            }
        } else {
            node.to_owned()
        }
    }

    fill_output_blocks_node(&info.ast, outputs)
}
