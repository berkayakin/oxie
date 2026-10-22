// Collection literals and APIs: HashMap::from, HashSet::from, BTreeMap/BTreeSet, entry,
// set operations, retain, extend and iteration in a deterministic order.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

fn sorted<K: Ord + Clone, V: Clone>(m: &HashMap<K, V>) -> Vec<(K, V)> {
    let mut v: Vec<(K, V)> = m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn main() {
    let mut ages = HashMap::from([("ada", 36), ("bob", 25), ("cy", 41)]);
    ages.insert("di", 19);
    *ages.get_mut("bob").unwrap() += 1;
    if let Some(a) = ages.get("ada") {
        println!("ada is {}", a);
    }
    println!("{:?}", sorted(&ages));
    ages.retain(|_, age| *age > 20);
    println!("{} adults, has cy: {}", ages.len(), ages.contains_key("cy"));

    let a: HashSet<i32> = HashSet::from([1, 2, 3, 4]);
    let b = HashSet::from([3, 4, 5]);
    let mut both: Vec<_> = a.intersection(&b).copied().collect();
    both.sort();
    let mut either: Vec<_> = a.union(&b).copied().collect();
    either.sort();
    let mut only_a: Vec<_> = a.difference(&b).collect();
    only_a.sort();
    println!("{:?} {:?} {:?}", both, either, only_a);

    let mut tree = BTreeMap::new();
    for word in "the quick brown fox jumps over the lazy dog the end".split(' ') {
        *tree.entry(word).or_insert(0) += 1;
    }
    println!("{:?}", tree);
    let (first, count) = tree.iter().next().unwrap();
    println!("first {} x{}, last {:?}", first, count, tree.keys().last());
    let common: Vec<&str> = tree.iter().filter(|(_, &n)| n > 1).map(|(w, _)| *w).collect();
    println!("{:?}", common);

    let mut primes = BTreeSet::from([2, 3, 5, 7]);
    primes.extend([11, 13]);
    primes.remove(&2);
    println!("{:?} {}", primes, primes.contains(&7));
    let ranged: Vec<_> = primes.range(4..12).collect();
    println!("{:?}", ranged);

    let mut grid: HashMap<(i32, i32), char> = HashMap::new();
    grid.insert((0, 0), '#');
    grid.entry((1, 0)).or_insert('.');
    grid.entry((0, 0)).and_modify(|c| *c = '@').or_insert('?');
    println!("{:?}", sorted(&grid));

    let mut v = vec![3, 1, 4, 1, 5, 9, 2, 6];
    v.extend_from_slice(&[5, 3]);
    v.truncate(8);
    v.insert(0, 0);
    v.swap(1, 2);
    let removed = v.remove(3);
    println!("{:?} removed {} max {:?} min {:?}", v, removed, v.iter().max(), v.iter().min());
    println!("{:?} {:?}", v.first(), v.get(100));
    let (left, right) = v.split_at(3);
    println!("{:?} | {:?} | {}", left, right, v.contains(&9));
    let joined = vec!["a", "b", "c"].concat();
    let nested = vec![vec![1, 2], vec![3]].concat();
    println!("{} {:?}", joined, nested);
}
