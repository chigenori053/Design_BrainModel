use design_domain::{Architecture, DesignUnit, Layer};
use policy_engine::{Role, generalize_architecture};

#[test]
fn generalizes_architecture_into_layer_roles() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    architecture.add_design_unit(DesignUnit::with_layer(3, "Repository", Layer::Repository));

    let pattern = generalize_architecture(&architecture);

    assert_eq!(
        pattern.node_roles,
        vec![Role::LayerA, Role::LayerB, Role::LayerC]
    );
    assert_eq!(pattern.relation_structure.node_count, 3);
}
