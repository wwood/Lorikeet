use std::collections::{HashMap, HashSet, BTreeMap, BTreeSet};

#[derive(Debug,Clone)]
pub struct Haplotype {
    abundance: f64,
    parent_node_id: usize,
    variants: HashMap<i32, (String, f64)>,
    node_level: usize,
    node_id: usize,
}

impl Haplotype {

}


#[derive(Debug, Clone)]
pub struct Genotype {
    pub read_ids: HashSet<i32>,
    pub start_var_pos: usize,
    pub ordered_variants: HashMap<i32, String>,
    pub frequencies: Vec<f64>,
}

impl Genotype {
    pub fn start(position: usize) -> Genotype {
        Genotype {
            read_ids: HashSet::new(),
            start_var_pos: position,
            ordered_variants: HashMap::new(),
            frequencies: Vec::new(),
        }
    }

    pub fn new(&mut self, read_id: i32, position_map: &BTreeMap<i32, String>,
           variant_abundances: &HashMap<i32, BTreeMap<String, f64>>) {
        self.read_ids = HashSet::new();
        self.frequencies = Vec::new();
        self.ordered_variants = HashMap::new();
        self.read_ids.insert(read_id);
        for (pos, variant) in position_map.iter() {
            let variant_map = &variant_abundances.get(pos);
            match variant_map {
                Some(variant_map) => {
                    debug!("Fetching position: {:?} {:?} {:?}", pos, variant_map, variant);
//                self.frequencies.push(variant_map[variant].clone());
                    self.ordered_variants
                        .insert(pos.clone(), variant.to_string());
                },
                None => continue,
            }
        }
    }

    pub fn check_variants(&self, position_map: &BTreeMap<i32, String>, intersection: Vec<i32>) -> bool {
        // check variants against stored variants for a genotype along the shared positions
        let mut new_var= false;
        for check_pos in intersection.iter() {
            if self.ordered_variants.contains_key(&check_pos) {
                let current_var = match self
                    .ordered_variants
                    .get(&check_pos) {
                    Some(var) => var,
                    None => {
                        println!("Position not recorded in variant map");
                        std::process::exit(1)
                    }
                };
                let check_var = position_map.get(check_pos)
                    .expect("No variant at that position while checking genotypes");

                if current_var != check_var {
                    //Then this is a new genotype
                    new_var = true;
                    break
                } else {
                    new_var = false;
                }
            } else {
                // current variant does not contain this position, must be new genotype
                new_var = true;
                break
            }
        }
        return new_var
    }
}