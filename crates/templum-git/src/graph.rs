//! History's graph: each commit's lane and the lines into and out of its row, from the commits
//! (children before parents: `--date-order`) and their parents. Pure: tested on lists.
//! See docs/specs/git.md, section 6.

use crate::parse::Commit;

/// One row of the graph. A line is (its lane at one edge, its lane at the other): `up` from the
/// row's top to its middle, `down` from its middle to its bottom.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
    /// The commit's lane: where its dot is.
    pub lane: usize,
    pub up: Vec<(usize, usize)>,
    pub down: Vec<(usize, usize)>,
    /// The lanes the row draws in (its widest one, plus one).
    pub width: usize,
}

/// The first free lane, or a new one.
fn free(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

/// The rows for `commits`, newest first. Each lane waits for the commit it leads to. A commit
/// takes the first lane waiting for it (a branch's tip, which none waits for, the first free
/// one); the other lanes waiting for it join it there. Its first parent goes on in its lane,
/// each other parent into a lane already waiting for it, else a free one. Every call starts from
/// nothing, so Load more (the whole list again) leaves the earlier rows as they were.
pub fn graph(commits: &[Commit]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for c in commits {
        let me = c.hash.as_str();
        let lane = match lanes.iter().position(|l| *l == Some(me)) {
            Some(i) => i,
            None => free(&mut lanes),
        };
        let mut row = GraphRow {
            lane,
            ..GraphRow::default()
        };
        // The lanes passing by this commit, waiting for another one.
        let mut passing = Vec::new();
        for (i, l) in lanes.iter().enumerate() {
            match *l {
                Some(h) if h == me => row.up.push((i, lane)),
                Some(_) => {
                    row.up.push((i, i));
                    passing.push(i);
                }
                None => {}
            }
        }
        for l in lanes.iter_mut() {
            if *l == Some(me) {
                *l = None;
            }
        }
        let mut parents = c.parents.iter().map(String::as_str);
        lanes[lane] = parents.next();
        if lanes[lane].is_some() {
            row.down.push((lane, lane));
        }
        for p in parents {
            let to = match lanes.iter().position(|l| *l == Some(p)) {
                Some(i) => i,
                None => {
                    let i = free(&mut lanes);
                    lanes[i] = Some(p);
                    i
                }
            };
            row.down.push((lane, to));
        }
        row.down.extend(passing.iter().map(|&i| (i, i)));
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        row.width = row
            .up
            .iter()
            .chain(&row.down)
            .flat_map(|&(a, b)| [a, b])
            .chain([lane])
            .max()
            .unwrap_or(0)
            + 1;
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.to_string(),
            short: hash.to_string(),
            author: String::new(),
            email: String::new(),
            time: 0,
            date: String::new(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            refs: Vec::new(),
            subject: hash.to_string(),
            body: String::new(),
        }
    }

    fn row(lane: usize, up: &[(usize, usize)], down: &[(usize, usize)], width: usize) -> GraphRow {
        GraphRow {
            lane,
            up: up.to_vec(),
            down: down.to_vec(),
            width,
        }
    }

    #[test]
    fn a_straight_line() {
        let g = graph(&[c("c3", &["c2"]), c("c2", &["c1"]), c("c1", &[])]);
        assert_eq!(
            g,
            [
                row(0, &[], &[(0, 0)], 1),
                row(0, &[(0, 0)], &[(0, 0)], 1),
                row(0, &[(0, 0)], &[], 1)
            ]
        );
    }

    #[test]
    fn a_branch_and_its_merge() {
        let g = graph(&[
            c("m", &["a", "b"]),
            c("a", &["r"]),
            c("b", &["r"]),
            c("r", &[]),
        ]);
        assert_eq!(
            g,
            [
                row(0, &[], &[(0, 0), (0, 1)], 2),
                row(0, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)], 2),
                row(1, &[(0, 0), (1, 1)], &[(1, 1), (0, 0)], 2),
                row(0, &[(0, 0), (1, 0)], &[], 2),
            ]
        );
    }

    #[test]
    fn an_octopus_merge() {
        let g = graph(&[
            c("m", &["p1", "p2", "p3"]),
            c("p1", &["r"]),
            c("p2", &["r"]),
            c("p3", &["r"]),
            c("r", &[]),
        ]);
        assert_eq!(g[0], row(0, &[], &[(0, 0), (0, 1), (0, 2)], 3));
        assert_eq!(g[3].lane, 2);
        assert_eq!(
            g[4],
            row(0, &[(0, 0), (1, 0), (2, 0)], &[], 3),
            "three lanes join at the root"
        );
    }

    #[test]
    fn a_root_commit_in_the_middle() {
        // Two unrelated histories by date: y1 has no parent and nothing leads to it.
        let g = graph(&[c("x2", &["x1"]), c("y1", &[]), c("x1", &[])]);
        assert_eq!(
            g,
            [
                row(0, &[], &[(0, 0)], 1),
                row(1, &[(0, 0)], &[(0, 0)], 2),
                row(0, &[(0, 0)], &[], 1)
            ]
        );
    }

    #[test]
    fn two_tips_on_one_base() {
        let g = graph(&[c("t1", &["base"]), c("t2", &["base"]), c("base", &[])]);
        assert_eq!(
            g[1],
            row(1, &[(0, 0)], &[(1, 1), (0, 0)], 2),
            "a new lane for the second tip"
        );
        assert_eq!(g[2], row(0, &[(0, 0), (1, 0)], &[], 2));
    }

    #[test]
    fn a_graph_is_the_same_for_a_list_and_its_start() {
        // Load more passes the whole list: the first page's rows don't change.
        let all = [
            c("m", &["a", "b"]),
            c("a", &["r"]),
            c("b", &["r"]),
            c("r", &[]),
        ];
        assert_eq!(graph(&all[..2]), graph(&all)[..2]);
    }
}
