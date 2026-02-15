# Objective Overlap Report

ObjectiveFeatureMap (inferred from current evaluator implementation):
- Cost: edge_count, history_length, node_count, resource_proxy
- Performance: edge_count, fanout_proxy, field_resonance, structural_density
- Reliability: chm_risk, constraint_proxy, edge_count, redundancy_proxy
- Structure: connectivity_proxy, depth_proxy, edge_count, node_count
- Field: category_basis, field_projection, target_alignment

Pairwise Jaccard overlap:
- Cost vs Performance: overlap=0.143 (ok)
- Cost vs Reliability: overlap=0.143 (ok)
- Cost vs Structure: overlap=0.333 (ok)
- Cost vs Field: overlap=0.000 (ok)
- Performance vs Reliability: overlap=0.143 (ok)
- Performance vs Structure: overlap=0.143 (ok)
- Performance vs Field: overlap=0.000 (ok)
- Reliability vs Structure: overlap=0.143 (ok)
- Reliability vs Field: overlap=0.000 (ok)
- Structure vs Field: overlap=0.000 (ok)

Contraction decision (Base variant):
- collapse_depth_count=0/100 (0.000)
- high_|corr|>0.9 multi-pair depths=69
- objective-space contraction: NOT_CONFIRMED
