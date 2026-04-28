#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderStage {
    Background,
    Blur,
    Surfaces,
    Decorations,
    Cursor,
    Overlay,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderResource {
    pub name: String,
}

impl RenderResource {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderNodeDescriptor {
    pub name: String,
    pub stage: RenderStage,
    pub reads: Vec<RenderResource>,
    pub writes: Vec<RenderResource>,
}

impl RenderNodeDescriptor {
    pub fn new(name: impl Into<String>, stage: RenderStage) -> Self {
        Self {
            name: name.into(),
            stage,
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    pub fn reads(mut self, resources: impl IntoIterator<Item = RenderResource>) -> Self {
        self.reads.extend(resources);
        self
    }

    pub fn writes(mut self, resources: impl IntoIterator<Item = RenderResource>) -> Self {
        self.writes.extend(resources);
        self
    }
}

#[derive(Default)]
pub struct RenderGraph {
    nodes: Vec<RenderNodeDescriptor>,
}

impl RenderGraph {
    pub fn add_node(&mut self, node: RenderNodeDescriptor) {
        self.nodes.push(node);
        self.nodes.sort_by_key(|node| node.stage);
    }

    pub fn nodes(&self) -> &[RenderNodeDescriptor] {
        &self.nodes
    }

    pub fn stage_nodes(&self, stage: RenderStage) -> impl Iterator<Item = &RenderNodeDescriptor> {
        self.nodes.iter().filter(move |node| node.stage == stage)
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderGraph, RenderNodeDescriptor, RenderStage};

    #[test]
    fn graph_orders_nodes_by_stage() {
        let mut graph = RenderGraph::default();
        graph.add_node(RenderNodeDescriptor::new(
            "decorations",
            RenderStage::Decorations,
        ));
        graph.add_node(RenderNodeDescriptor::new("surfaces", RenderStage::Surfaces));

        let names: Vec<_> = graph
            .nodes()
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        assert_eq!(names, vec!["surfaces", "decorations"]);
    }
}
