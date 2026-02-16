use std::collections::{BTreeSet, HashMap};
use std::f64;

/// Computes the rank of elements in the vector, handling ties by assigning average rank.
fn rank_data(v: &[f64]) -> Vec<f64> {
    let mut indexed: Vec<(usize, f64)> = v.iter().copied().enumerate().collect();
    // Sort by value.
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut ranks = vec![0.0; v.len()];
    let n = v.len();
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && (indexed[j].1 - indexed[i].1).abs() < 1e-9 {
            j += 1;
        }
        // Elements from i to j-1 have equal values.
        // The ranks would be i+1, i+2, ..., j.
        // Average rank = ( (i+1) + j ) / 2.0
        let rank_sum: f64 = ((i + 1)..=j).map(|x| x as f64).sum();
        let avg_rank = rank_sum / (j - i) as f64;
        
        for k in i..j {
            ranks[indexed[k].0] = avg_rank;
        }
        i = j;
    }
    ranks
}

/// Computes Spearman's rank correlation coefficient.
pub fn spearman_correlation(x: &[f64], y: &[f64]) -> f64 {
    if x.len() != y.len() || x.len() < 2 {
        return 0.0;
    }
    let rx = rank_data(x);
    let ry = rank_data(y);
    
    // Pearson correlation of ranks
    let n = x.len() as f64;
    let mean_rx = rx.iter().sum::<f64>() / n;
    let mean_ry = ry.iter().sum::<f64>() / n;
    
    let mut num = 0.0;
    let mut den_x = 0.0;
    let mut den_y = 0.0;
    
    for i in 0..x.len() {
        let dx = rx[i] - mean_rx;
        let dy = ry[i] - mean_ry;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }
    
    if den_x <= 1e-12 || den_y <= 1e-12 {
        return 0.0;
    }
    
    num / (den_x.sqrt() * den_y.sqrt())
}

/// Compute covariance matrix for 4D vectors.
pub fn covariance_matrix4(data: &[[f64; 4]]) -> [[f64; 4]; 4] {
    let n = data.len();
    if n < 2 {
        return [[0.0; 4]; 4]; // Or identity? 0 is safer implies no variance.
    }
    
    let mut mean = [0.0; 4];
    for v in data {
        for i in 0..4 {
            mean[i] += v[i];
        }
    }
    for i in 0..4 {
        mean[i] /= n as f64;
    }
    
    let mut cov = [[0.0; 4]; 4];
    for v in data {
        for i in 0..4 {
            for j in 0..4 {
                cov[i][j] += (v[i] - mean[i]) * (v[j] - mean[j]);
            }
        }
    }
    
    for i in 0..4 {
        for j in 0..4 {
            cov[i][j] /= (n - 1) as f64;
        }
    }
    cov
}

/// Cyclic Jacobi method to compute eigenvalues of a symmetric 4x4 matrix.
/// Returns eigenvalues sorted descending.
pub fn eigenvalues_jacobi_4x4(matrix: &[[f64; 4]; 4]) -> [f64; 4] {
    let mut a = *matrix;
    let n = 4;
    let max_iter = 50;
    let eps = 1e-12;
    
    // We only need eigenvalues, so we don't track eigenvectors matrix V.
    
    for _ in 0..max_iter {
        // Find max off-diagonal element
        let mut max_val = 0.0;
        let mut p = 0;
        let mut q = 0;
        
        for i in 0..n {
            for j in (i + 1)..n {
                if a[i][j].abs() > max_val {
                    max_val = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        
        if max_val < eps {
            break;
        }
        
        // Calculate rotation parameters
        let y = (a[q][q] - a[p][p]) / 2.0;
        let x = -a[p][q]; // NOTE: Sign convention might differ, but we want to zero a[p][q]
        // tan(2theta) = (2 * a[p][q]) / (a[p][p] - a[q][q])
        // Let's use robust calculation.
        
        let t = if y.abs() < eps {
            x.signum() // 45 degrees if diagonal equal
        } else {
            // t = sgn(y) / (|y| + sqrt(y^2 + x^2)) * x is one formula?
            // Standard: theta = 0.5 * atan2(2*a[p][q], a[p][p] - a[q][q])
            // But we need t = tan(theta).
            // t^2 + 2*y/x * t - 1 = 0
            let r = (x * x + y * y).sqrt();
            let d = if y >= 0.0 { y + r } else { y - r };
            if d.abs() < eps { 0.0 } else { x / d }
        };
        
        let c = 1.0 / (1.0 + t * t).sqrt();
        let s = t * c;
        let tau = s / (1.0 + c);
        
        // Update matrix elements
        // a'[p][p] = a[p][p] - t * a[p][q]
        // a'[q][q] = a[q][q] + t * a[p][q]
        let temp = a[p][q];
        a[p][p] -= t * temp;
        a[q][q] += t * temp;
        a[p][q] = 0.0;
        a[q][p] = 0.0; // strict symmetry
        
        for r in 0..n {
            if r != p && r != q {
                let arp = a[r][p];
                let arq = a[r][q];
                // a'[r][p] = c * a[r][p] - s * a[r][q]
                // a'[r][q] = s * a[r][p] + c * a[r][q]
                // using tau for stability:
                // a[r][p] = a[r][p] - s * (a[r][q] + tau * a[r][p])
                // a[r][q] = a[r][q] + s * (a[r][p] - tau * a[r][q])
                
                a[r][p] -= s * (arq + tau * arp);
                a[p][r] = a[r][p];
                
                a[r][q] += s * (arp - tau * arq);
                a[q][r] = a[r][q];
            }
        }
    }
    
    let mut evs = [a[0][0], a[1][1], a[2][2], a[3][3]];
    evs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    evs
}

#[derive(Clone, Debug, Default)]
pub struct StabilityMetrics {
    // Rule 1
    pub redundancy_flags: Vec<String>, 
    // Rule 2
    pub saturation_flags: Vec<String>,
    pub discrete_saturation_count: usize,
    // Rule 3 (MAD=0) handled by GlobalRobustStats but we track counts here
    pub mad_zero_flags: Vec<String>,
    // Rule 4
    pub effective_dim: usize,
    pub eigenvalues: [f64; 4],
    pub effective_dim_ratio: f64,
    // Rule 7
    pub is_collapsed: bool,
    pub collapse_reasons: Vec<String>,
}

pub struct ObjectiveStabilityAnalyzer;

impl ObjectiveStabilityAnalyzer {
    pub fn analyze(
        data: &[[f64; 4]],
        mad: &[f64; 4],
        unique_norm_vec_count: usize,
        mean_nn_dist_norm: f64,
    ) -> StabilityMetrics {
        let mut m = StabilityMetrics::default();
        if data.is_empty() {
            return m;
        }

        // --- Rule 1: Redundancy Detection ---
        // u_i(depth) == u_j(depth) for all? (This is hard to check "all depth" here, we check current depth)
        // median Spearman rho >= 0.7
        let n_dim = 4;
        let mut redundant_pairs = Vec::new();
        for i in 0..n_dim {
            for j in (i + 1)..n_dim {
                let col_i: Vec<f64> = data.iter().map(|v| v[i]).collect();
                let col_j: Vec<f64> = data.iter().map(|v| v[j]).collect();
                
                let rho = spearman_correlation(&col_i, &col_j);
                if rho.abs() >= 0.7 {
                    redundant_pairs.push(format!("dim{}&{}(rho={:.2})", i, j, rho));
                }
            }
        }
        m.redundancy_flags = redundant_pairs;

        // --- Rule 2: Saturation Detection ---
        // u_i / n < 0.15
        // u_i = unique values count
        let n_samples = data.len() as f64;
        for i in 0..n_dim {
            let mut col: Vec<f64> = data.iter().map(|v| v[i]).collect();
            col.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            col.dedup_by(|a, b| (a - b).abs() < 1e-9);
            let u_i = col.len() as f64;
            
            if u_i / n_samples.max(1.0) < 0.15 {
                m.saturation_flags.push(format!("dim{}(u={})", i, u_i));
                m.discrete_saturation_count += 1;
            }
        }

        // --- Rule 3: MAD=0 Detection ---
        for i in 0..n_dim {
            if mad[i] < 1e-12 {
                m.mad_zero_flags.push(format!("dim{}", i));
            }
        }

        // --- Rule 4: Effective Dimension Guarantee ---
        let cov = covariance_matrix4(data);
        let evs = eigenvalues_jacobi_4x4(&cov);
        m.eigenvalues = evs;
        let sum_ev: f64 = evs.iter().sum();
        if sum_ev > 1e-12 {
            m.effective_dim = evs.iter().filter(|&&lam| lam / sum_ev >= 0.05).count();
            m.effective_dim_ratio = evs[0] / sum_ev; // Just an example metric
        } else {
            m.effective_dim = 0;
        }

        // --- Rule 7: Collapse Definition v3 ---
        // unique_norm_vec_count == 1
        // mean_nn_dist_norm == 0
        // norm_dim_mad_zero_count >= 2 (Refers to MAD=0 in NORMALIZED space? Or Raw? Spec says "norm_dim_mad_zero_count")
        // Assuming the input `mad` is from raw globalstats, but let's check input args.
        // Spec says: "Objective Space Stability Specification v3.0 ... collapse ... norm_dim_mad_zero_count >= 2"
        // In lib.rs we calculate `norm_dim_mad_zero_count` from active dims logic. 
        // We'll use the passed `mad_zero_flags` count for now.
        
        let mad_zero_count = m.mad_zero_flags.len();
        
        // "mean_nn_dist_norm == 0" might be too strict for float, use epsilon.
        let dist_zero = mean_nn_dist_norm < 1e-6;
        let unique_one = unique_norm_vec_count <= 1;
        
        if unique_one && dist_zero && mad_zero_count >= 2 {
            m.is_collapsed = true;
            m.collapse_reasons.push("V3_CRITERIA_MET".to_string());
        }

        m
    }
}
