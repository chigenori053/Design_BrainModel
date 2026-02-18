# Phase3 Post Validation (depth=50, same seed)

## Conditions
- depth: 50
- beams: [5, 8, 12]
- seed (fixed): 42
- norm-alpha (fixed): 0.25
- category-alpha (fixed): 3.0
- filters: `--baseline-off --category-soft`

Beam5:
  u0==u3 ratio: 0.9600
  median rho: 0.0000
  mean rho: 0.0000
  MAD0_o1: 0.9800
  MAD0_o2: 0.9800
  avg rank: 0.1200
  avg unique_distance_ratio: 0.0216

Beam8:
  u0==u3 ratio: 0.9600
  median rho: 0.0000
  mean rho: 0.0000
  MAD0_o1: 0.9800
  MAD0_o2: 0.9800
  avg rank: 0.1200
  avg unique_distance_ratio: 0.0206

Beam12:
  u0==u3 ratio: 0.9600
  median rho: 0.0000
  mean rho: 0.0000
  MAD0_o1: 0.9800
  MAD0_o2: 0.9800
  avg rank: 0.1200
  avg unique_distance_ratio: 0.0203

## Stage Saturation (depth average)
| Beam | n | u0 | u0/n | tie_rate |
|---:|---:|---:|---:|---:|
| 5 | 77.6800 | 1.6800 | 0.0374 | 0.9626 |
| 8 | 115.7800 | 1.6800 | 0.0315 | 0.9685 |
| 12 | 173.9000 | 1.6800 | 0.0276 | 0.9724 |

## Front Thickness / Tie Rate
- Beam5: avg_front_thickness=0.0172, avg_tie_rate=0.9626
- Beam8: avg_front_thickness=0.0172, avg_tie_rate=0.9685
- Beam12: avg_front_thickness=0.0172, avg_tie_rate=0.9724

## Criteria Check
- Beam8 -> Beam12 で u0増加: NO
- unique_distance_ratio 低下 (Beam5>Beam8>Beam12): YES (values=0.0216, 0.0206, 0.0203)
- Beam5 collapse継続条件(部分): YES
- Beam8 collapse継続条件(部分): YES
- Beam12 collapse継続条件(部分): YES
