use std::str;

#[derive(Clone, Copy, PartialEq)]
enum NodeRef {
    Leaf(u32),
    Inner(u32),
}

struct Node {
    begin: u32,
    end: u32,
    suffix: NodeRef,
    child: [NodeRef; 256],
}

struct Cursor {
    node: u32,
    pos: u32,
}

struct SuffixTree<'a> {
    payload: &'a [u8],
    nodes: Vec<Node>,
}

impl<'a> SuffixTree<'a> {
    fn update(&mut self, cur_in: Cursor, new_end: u32) -> Cursor {
        let mut cur = cur_in;
        let new_ch = self.payload[new_end as usize];
        let mut prev_insert_idx: u32 = 0;

        loop {
            // Canonicalize active point
            while cur.pos < new_end {
                let link_ch = self.payload[cur.pos as usize];
                match self.nodes[cur.node as usize].child[link_ch as usize] {
                    NodeRef::Leaf(_) => {
                        // Leafs can absorb the entire rest of the string, and we can't
                        // descend into them; nothing to do.
                        break;
                    }
                    NodeRef::Inner(idx_u32) => {
                        let idx = idx_u32 as usize;
                        debug_assert!(idx != 0, "canonicalize should only follow real links");
                        let len = self.nodes[idx].end - self.nodes[idx].begin;
                        if len > new_end - cur.pos {
                            // Label of this inner node extends past the characters
                            // we currently have; we can't follow a link further, so
                            // we're done!
                            break;
                        }

                        // Descent into this node and keep going
                        cur.node = idx_u32;
                        cur.pos += len;
                    }
                }
            }

            // Do we have an outgoing link with the new character already?
            let insert_node_idx = if cur.pos == new_end {
                // Would insert right below active node; do we have
                // a link for this character already?
                if self.nodes[cur.node as usize].child[new_ch as usize] != NodeRef::Inner(0) {
                    // We have this already; nothing to do for now!
                    break;
                } else {
                    // Insert right below current node.
                    cur.node
                }
            } else {
                // We're in the middle of a longer label; check whether we have a
                // mismatch (in which case we need to split) or not.
                let edge_select_ch = self.payload[cur.pos as usize] as usize;
                let edge_ref = self.nodes[cur.node as usize].child[edge_select_ch as usize];

                // For us to get here, this reference should exist
                debug_assert!(edge_ref != NodeRef::Inner(0));

                let edge_label_begin = match edge_ref {
                    NodeRef::Leaf(pos) => pos,
                    NodeRef::Inner(idx) => self.nodes[idx as usize].begin
                };
                let cur_label_pos = edge_label_begin + new_end - cur.pos;
                let cur_label_ch = self.payload[cur_label_pos as usize];

                // Do we match the next character of the edge label or not?
                if new_ch == cur_label_ch {
                    // We do; nothing to do for now!
                    break;
                } else {
                    // We don't, so we need to split this edge
                    let mut n = Node {
                        begin: edge_label_begin,
                        end: cur_label_pos,
                        suffix: NodeRef::Inner(1),
                        child: [NodeRef::Inner(0); 256]
                    };
                    // Transfer over the existing node as first child
                    n.child[cur_label_ch as usize] = match edge_ref {
                        NodeRef::Leaf(_) => NodeRef::Leaf(cur_label_pos),
                        NodeRef::Inner(idx_u32) => {
                            let idx = idx_u32 as usize;
                            // Update the inner node to shorten its edge label
                            debug_assert!(self.nodes[idx].begin < cur_label_pos && cur_label_pos < self.nodes[idx].end);
                            self.nodes[idx].begin = cur_label_pos;
                            NodeRef::Inner(idx_u32)
                        }
                    };
                    // Insert the new node and remember its index
                    let new_node_idx = self.nodes.len() as u32;
                    self.nodes.push(n);
                    // Link in the newly create node right below the active node
                    self.nodes[cur.node as usize].child[edge_select_ch as usize] = NodeRef::Inner(new_node_idx);
                    // Return the index of the newly created node
                    new_node_idx
                }
            };

            // Update the suffix links
            self.nodes[prev_insert_idx as usize].suffix = NodeRef::Inner(insert_node_idx);
            prev_insert_idx = insert_node_idx;

            // Add the new leaf
            debug_assert!(self.nodes[insert_node_idx as usize].child[new_ch as usize] == NodeRef::Inner(0));
            self.nodes[insert_node_idx as usize].child[new_ch as usize] = NodeRef::Leaf(new_end);

            // Continue on to the next suffix
            if let NodeRef::Inner(idx_u32) = self.nodes[cur.node as usize].suffix {
                cur.node = idx_u32;
            } else {
                panic!("Suffix links must be to inner nodes!");
            }
        }

        cur
    }

    fn print_rec(&self, node: NodeRef, indent: usize, cur_end: u32)
    {
        print!("{:1$}", "", indent*2);
        match node {
            NodeRef::Inner(idx_u32) => {
                let n = &self.nodes[idx_u32 as usize];
                if idx_u32 == 1 {
                    println!("(root)");
                } else {
                    let suffix_ind = match n.suffix {
                        NodeRef::Inner(idx) => idx,
                        _ => 0,
                    };
                    println!("\"{}\" (inner {}, suffix={})",
                        str::from_utf8(&self.payload[n.begin as usize..n.end as usize]).unwrap(),
                        idx_u32, suffix_ind);
                }
                for r in n.child.iter() {
                    match r {
                        NodeRef::Inner(0) => {},
                        NodeRef::Inner(_) => self.print_rec(*r, indent + 1, cur_end),
                        NodeRef::Leaf(_) => self.print_rec(*r, indent + 1, cur_end),
                    }
                }
            },
            NodeRef::Leaf(pos) => {
                println!("\"{}\" (leaf)", str::from_utf8(&self.payload[pos as usize..cur_end as usize]).unwrap());
            },
        }
    }

    fn print(&self) {
        self.print_rec(NodeRef::Inner(1), 0, self.payload.len() as u32);
    }

    fn from(payload: &'a [u8]) -> SuffixTree<'a> {
        let len32 = payload.len() as u32; // check here!
        let mut st = SuffixTree { payload: payload, nodes: Vec::new() };

        // Add the two sentinel nodes
        // Top is node 0
        st.nodes.push(Node { begin: 0, end: 0, suffix: NodeRef::Inner(0), child: [NodeRef::Inner(1); 256] });
        // Root is node 1
        st.nodes.push(Node { begin: 0, end: 1, suffix: NodeRef::Inner(0), child: [NodeRef::Inner(0); 256] });

        let mut curs = Cursor { node: 1, pos: 0 };

        // We actually do need to iterate over the indices, not the elements of payload, here
        for i in 0..len32 {
            curs = st.update(curs, i)
        }

        st
    }
}

fn main() {
    let payload = "bananas$".as_bytes();
    let st = SuffixTree::from(payload);
    st.print();
}
