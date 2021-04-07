// QuiZX - Rust library for quantum circuit rewriting and optimisation
//         using the ZX-calculus
// Copyright (C) 2021 - Aleks Kissinger
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::graph::*;
use crate::basic_rules::*;
use rustc_hash::FxHashMap;
use num::{Rational,Zero};

/// Repeatedly apply the given rule at any vertex
/// that matches the check function
///
/// We assume the rule will at most delete the current
/// vertex, and leave other vertices in place (although
/// edges might change).
pub fn vertex_simp<G: GraphLike>(
    g: &mut G,
    check: fn(&G, V) -> bool,
    rule: fn(&mut G, V) -> (),
    force_reduce: bool
    ) -> bool
{
    let mut got_match = false;
    let mut new_matches = true;
    let mut numv;
    while new_matches {
        numv = g.num_vertices();
        new_matches = false;
        for v in g.vertex_vec() {
            if check(g, v) {
                rule(g, v);
                new_matches = true;
                got_match = true;
            }
        }
        if force_reduce && numv >= g.num_vertices() { break; }
    }

    got_match
}

pub fn edge_simp<G: GraphLike>(
    g: &mut G,
    edge_type: EType,
    check: fn(&G, V, V) -> bool,
    rule: fn(&mut G, V, V) -> (),
    force_reduce: bool
    ) -> bool
{
    let mut got_match = false;
    let mut new_matches = true;
    let mut numv;
    while new_matches {
        numv = g.num_vertices();
        new_matches = false;
        for (s,t,et) in g.edge_vec() {
            if et != edge_type ||
               !g.contains_vertex(s) ||
               !g.contains_vertex(t) ||
               !check(g, s, t)
               { continue; }
            rule(g, s, t);
            new_matches = true;
            got_match = true;
        }
        if force_reduce && numv >= g.num_vertices() { break; }
    }

    got_match
}

pub fn id_simp(g: &mut impl GraphLike) -> bool {
    vertex_simp(g, check_remove_id, remove_id_unchecked, false)
}

pub fn local_comp_simp(g: &mut impl GraphLike) -> bool {
    vertex_simp(g, check_local_comp, local_comp_unchecked, false)
}

pub fn spider_simp(g: &mut impl GraphLike) -> bool {
    edge_simp(g, EType::N, check_spider_fusion, spider_fusion_unchecked, false)
}

pub fn pivot_simp(g: &mut impl GraphLike) -> bool {
    edge_simp(g, EType::H, check_pivot, pivot_unchecked, false)
}

pub fn gen_pivot_simp(g: &mut impl GraphLike) -> bool {
    edge_simp(g, EType::H, check_gen_pivot_reduce, gen_pivot_unchecked, false)
}

pub fn interior_clifford_simp(g: &mut impl GraphLike) -> bool {
    spider_simp(g);
    g.x_to_z();
    let mut got_match = false;
    let mut m = true;
    while m {
        m = id_simp(g)
         || spider_simp(g)
         || pivot_simp(g)
         || local_comp_simp(g);
        if m { got_match = true; }
    }

    got_match
}

pub fn clifford_simp(g: &mut impl GraphLike) -> bool {
    let mut got_match = false;
    let mut m = true;
    while m {
        // let numv = g.num_vertices();
        // println!("v: {}", numv);
        m = interior_clifford_simp(g) ||
            gen_pivot_simp(g);
        if m { got_match = true; }
        // if !(g.num_vertices() < numv) { break; }
    }

    got_match
}

pub fn fuse_gadgets(g: &mut impl GraphLike) -> bool {
    let mut gadgets: FxHashMap<Vec<V>,Vec<(V,V)>> = FxHashMap::default();

    for v in g.vertices() {
        if g.vertex_type(v) != VType::Z ||
           !g.phase(v).is_zero() { continue; }
        if g.degree(v) == 1 {
            let w = g.neighbors(v).next().unwrap();
            let mut nhd = Vec::new();
            for (n,et) in g.incident_edges(w) {
                if g.vertex_type(n) != VType::Z ||
                   et != EType::H { continue; }
                if n != v { nhd.push(v); }
            }
            nhd.sort();

            if let Some(gs) = gadgets.get_mut(&nhd) {
                gs.push((w, v));
            } else {
                gadgets.insert(nhd, vec![(w,v)]);
            }
        }
    }

    let mut fused = false;
    for gs in gadgets.values() {
        if gs.len() > 1 {
            fused = true;
            let mut it = gs.iter(); it.next();
            let mut ph = Rational::zero();
            for i in 1..gs.len() {
                ph += g.phase(gs[i].1);
                g.remove_vertex(gs[i].0);
                g.remove_vertex(gs[i].1);
            }

            g.add_to_phase(gs[0].1, ph);
        }
    }

    fused
}

pub fn full_simp(g: &mut impl GraphLike) -> bool {
    let mut got_match = false;
    let mut m = true;
    while m {
        m = clifford_simp(g)
         || fuse_gadgets(g);
        if m { got_match = true; }
    }

    got_match
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::*;
    use crate::vec_graph::Graph;
    use crate::tensor::ToTensor;

    #[test]
    fn simp_cnot() {
        let c = Circuit::from_qasm(r#"
            qreg q[4];
            cx q[0], q[1];
            cx q[0], q[2];
            cx q[0], q[3];
            cx q[1], q[2];
            cx q[2], q[1];
            cx q[1], q[2];
            cx q[1], q[3];
            cx q[1], q[0];
        "#).unwrap();
        let mut g: Graph = c.to_graph();
        clifford_simp(&mut g);

        println!("{}", g.to_dot());
        assert_eq!(c.to_tensor4(), g.to_tensor4());
    }

    #[test]
    fn cliff_simp() {
        let c = Circuit::from_qasm(r#"
            qreg q[3];
            h q[1];
            h q[1];
            s q[0];
            cx q[0], q[1];
            cx q[1], q[0];
            cx q[0], q[1];
            s q[1];
            cx q[1], q[2];
            cx q[0], q[2];
            h q[2];
            z q[0];
        "#).unwrap();

        let g: Graph = c.to_graph();

        let mut h = g.clone();
        assert!(interior_clifford_simp(&mut h));
        assert_eq!(g.to_tensor4(), h.to_tensor4());

        let mut h = g.clone();
        assert!(clifford_simp(&mut h));
        println!("{}", g.to_dot());
        println!("{}", h.to_dot());
        assert_eq!(g.to_tensor4(), h.to_tensor4());
    }

    #[test]
    fn cliff_simp_2() {
        let c = Circuit::random()
            .seed(1337)
            .qubits(5)
            .depth(50)
            .p_t(0.2)
            .with_cliffords()
            .build();

        let g: Graph = c.to_graph();

        let mut h = g.clone();
        assert!(interior_clifford_simp(&mut h));
        assert_eq!(g.to_tensor4(), h.to_tensor4());

        let mut h = g.clone();
        assert!(clifford_simp(&mut h));
        println!("{}", g.to_dot());
        println!("{}", h.to_dot());
        assert_eq!(g.to_tensor4(), h.to_tensor4());
    }
}