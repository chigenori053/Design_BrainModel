use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{DesignEdge, DesignNode, DesignNodeId, DesignNodeKind};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeRef {
    Primitive(String),
    Custom(String),
    List(Box<TypeRef>),
    Optional(Box<TypeRef>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldSpec {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MethodSpec {
    pub name: String,
    pub inputs: Vec<TypeRef>,
    pub output: Option<TypeRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InterfaceSpec {
    pub name: String,
    pub methods: Vec<MethodSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructSpec {
    pub name: String,
    pub fields: Vec<FieldSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignGraph {
    nodes: Arc<Vec<DesignNode>>,
    edges: Arc<Vec<DesignEdge>>,
    node_index: Arc<BTreeMap<DesignNodeId, usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplementationUnit {
    pub module_name: String,
    pub dependencies: Vec<String>,
    pub public_interfaces: Vec<InterfaceSpec>,
    pub internal_structs: Vec<StructSpec>,
    pub language_hint: Option<String>,
    pub annotations: Vec<String>,
}

impl Default for DesignGraph {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new())
    }
}

impl DesignGraph {
    pub fn new(nodes: Vec<DesignNode>, edges: Vec<DesignEdge>) -> Self {
        let node_index = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), index))
            .collect();
        Self {
            nodes: Arc::new(nodes),
            edges: Arc::new(edges),
            node_index: Arc::new(node_index),
        }
    }

    pub fn nodes(&self) -> &[DesignNode] {
        self.nodes.as_slice()
    }

    pub fn edges(&self) -> &[DesignEdge] {
        self.edges.as_slice()
    }

    pub fn find_by_kind(&self, kind: DesignNodeKind) -> Vec<&DesignNode> {
        self.nodes.iter().filter(|node| node.kind == kind).collect()
    }

    pub fn dependencies(&self, node_id: DesignNodeId) -> Vec<DesignNodeId> {
        self.edges
            .iter()
            .filter(|edge| edge.source == node_id)
            .map(|edge| edge.target.clone())
            .collect()
    }

    pub fn node(&self, node_id: &DesignNodeId) -> Option<&DesignNode> {
        self.node_index
            .get(node_id)
            .and_then(|index| self.nodes.get(*index))
    }

    pub fn to_implementation_units(&self) -> Vec<ImplementationUnit> {
        self.nodes
            .iter()
            .map(|node| ImplementationUnit {
                module_name: node.name.clone(),
                dependencies: self
                    .dependencies(node.id.clone())
                    .into_iter()
                    .filter_map(|dependency| self.node(&dependency).map(|node| node.name.clone()))
                    .collect(),
                public_interfaces: interface_specs_for(node),
                internal_structs: struct_specs_for(node),
                language_hint: node.metadata.language_hint.clone(),
                annotations: node.metadata.annotations.clone(),
            })
            .collect()
    }
}

fn interface_specs_for(node: &DesignNode) -> Vec<InterfaceSpec> {
    let method_name = match node.kind {
        DesignNodeKind::API => "handle",
        DesignNodeKind::Service => "execute",
        DesignNodeKind::Database => "find",
        DesignNodeKind::Domain => "apply",
        DesignNodeKind::Interface => "call",
        DesignNodeKind::Module => "run",
    };
    vec![InterfaceSpec {
        name: format!("{}Interface", pascal_case(&node.name)),
        methods: vec![MethodSpec {
            name: method_name.to_string(),
            inputs: vec![TypeRef::Primitive("String".to_string())],
            output: Some(TypeRef::Optional(Box::new(TypeRef::Custom(format!(
                "{}Result",
                pascal_case(&node.name)
            ))))),
        }],
    }]
}

fn struct_specs_for(node: &DesignNode) -> Vec<StructSpec> {
    vec![StructSpec {
        name: format!("{}State", pascal_case(&node.name)),
        fields: vec![FieldSpec {
            name: "id".to_string(),
            ty: TypeRef::Primitive("String".to_string()),
        }],
    }]
}

fn pascal_case(value: &str) -> String {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    first.to_ascii_uppercase().to_string()
                        + &chars.as_str().to_ascii_lowercase()
                }
                None => String::new(),
            }
        })
        .collect()
}
