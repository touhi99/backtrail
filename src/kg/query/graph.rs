use super::answers::{links, strength};
use super::{claim_rows, line, tokens};
use crate::kg::entities;
use rusqlite::Connection;
use std::fmt::Write;

const FANOUT: [usize; 3] = [12, 5, 3];
const NEIGHBOR_BUDGET: usize = 2500;

pub fn neighbors(c: &Connection, name: &str, depth: usize) {
    let Some((id, ename)) = entities::find(c, name) else {
        println!("no record: no entity named '{name}'");
        return;
    };
    let depth = depth.clamp(1, 3);
    let mut seen: std::collections::HashSet<String> = [id.clone()].into();
    let mut frontier: Vec<(String, String)> = vec![(id, ename.clone())];
    let mut out =
        format!("{ename}, links up to {depth} hop(s) away (linked = a claim names both):\n");
    let mut cut_total = 0;
    let mut budget_hit = false;
    let mut found = 0;
    for (hop, &fanout) in FANOUT.iter().enumerate().take(depth) {
        let mut next: Vec<(String, String)> = vec![];
        let mut hop_out = String::new();
        for (fid, fname) in &frontier {
            let mut nb = links(c, fid);
            nb.retain(|n| !seen.contains(&n.0));
            nb.sort_by(|x, y| strength(y).total_cmp(&strength(x)));
            cut_total += nb.len().saturating_sub(fanout);
            for (oid, oname, k, _) in nb.into_iter().take(fanout) {
                if !seen.insert(oid.clone()) {
                    continue;
                }
                found += 1;
                let ev = claim_rows(
                    c,
                    "c.id IN (SELECT x.claim_id FROM claim_about x JOIN claim_about y ON y.claim_id=x.claim_id WHERE x.entity_id=?1 AND y.entity_id=?2) AND c.status!='retracted'",
                    &[fid, &oid],
                    1,
                );
                let mut l = format!("{}{} ({k} shared, via {fname})\n", "  ".repeat(hop), oname);
                if let Some(r) = ev.first() {
                    let _ = writeln!(
                        l,
                        "{}  {}",
                        "  ".repeat(hop),
                        line(r, 90).replace("\n    ", &format!("\n{}    ", "  ".repeat(hop)))
                    );
                }
                if tokens(&out) + tokens(&hop_out) + tokens(&l) > NEIGHBOR_BUDGET {
                    budget_hit = true;
                    cut_total += 1;
                    continue;
                }
                hop_out.push_str(&l);
                next.push((oid, oname));
            }
        }
        out.push_str(&hop_out);
        frontier = next;
    }
    if found == 0 {
        println!("no record: {ename} shares no claims with another entity");
        return;
    }
    print!("{out}");
    if cut_total > 0 {
        println!(
            "[{cut_total} more entities not shown{}; lowest shared-per-size first]",
            if budget_hit { " (token cap)" } else { "" }
        );
    }
    println!("[{} tokens]", tokens(&out));
}

const PATH_MAX_HOPS: usize = 5;

pub fn path(c: &Connection, a: &str, b: &str) {
    let (Some((aid, aname)), Some((bid, bname))) = (entities::find(c, a), entities::find(c, b))
    else {
        println!(
            "no record: no entity named '{}'",
            if entities::find(c, a).is_none() { a } else { b }
        );
        return;
    };
    if aid == bid {
        println!("{aname} and {bname} are the same entity");
        return;
    }
    let mut seen: std::collections::HashMap<String, (String, i64, String, f64)> =
        std::collections::HashMap::default();
    seen.insert(aid.clone(), (String::new(), 0, aname.clone(), 0.0));
    let mut frontier = vec![aid];
    let mut expanded = 0;
    for _ in 0..PATH_MAX_HOPS {
        let mut next: std::collections::HashMap<String, (String, i64, String, f64)> =
            std::collections::HashMap::default();
        for fid in &frontier {
            expanded += 1;
            let base = seen[fid].3;
            for n in links(c, fid) {
                if seen.contains_key(&n.0) {
                    continue;
                }
                let sc = base + strength(&n);
                if next.get(&n.0).is_none_or(|x| sc > x.3) {
                    next.insert(n.0.clone(), (fid.clone(), n.2, n.1.clone(), sc));
                }
            }
        }
        if next.is_empty() {
            break;
        }
        let found = next.contains_key(&bid);
        frontier = next.keys().cloned().collect();
        seen.extend(next);
        if found {
            let mut chain = vec![bid];
            while let Some(p) = seen
                .get(chain.last().expect("chain starts with one entry"))
                .map(|x| x.0.clone())
            {
                if p.is_empty() {
                    break;
                }
                chain.push(p);
            }
            chain.reverse();
            println!("{aname} to {bname}, {} link(s):", chain.len() - 1);
            for w in chain.windows(2) {
                let (from, to) = (&seen[&w[0]].2, &seen[&w[1]].2);
                println!("{from} - {to} ({} shared)", seen[&w[1]].1);
                for r in claim_rows(
                    c,
                    "c.id IN (SELECT x.claim_id FROM claim_about x JOIN claim_about y ON y.claim_id=x.claim_id WHERE x.entity_id=?1 AND y.entity_id=?2) AND c.status!='retracted'",
                    &[&w[0], &w[1]],
                    1,
                ) {
                    println!("  {}", line(&r, 110).replace("\n    ", "\n      "));
                }
            }
            return;
        }
    }
    println!(
        "no record: no chain of claims from {aname} to {bname} within {PATH_MAX_HOPS} links ({expanded} entities searched)"
    );
}
