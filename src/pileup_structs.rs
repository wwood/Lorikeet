use std::collections::{HashMap, HashSet};
use std::collections::BTreeMap;
use std::str;
use std::sync::{Arc, Mutex};
use std::io::prelude::*;
use rayon::prelude::*;
use permutation::*;
use codon_structs::*;
use bio::io::gff;



#[derive(Debug, Clone)]
pub struct Genotype {
    read_ids: HashSet<i32>,
    base_positions: Vec<i32>,
    start_var_pos: usize,
    ordered_variants: HashMap<i32, String>,
}

pub enum PileupStats {
    PileupContigStats {
        nucfrequency: Vec<HashMap<char, HashSet<i32>>>,
        variants_in_reads: HashMap<i32, BTreeMap<i32, String>>,
        variant_abundances: Vec<HashMap<String, f32>>,
        depth: Vec<usize>,
        indels: Vec<HashMap<String, HashSet<i32>>>,
        genotypes_per_position: HashMap<usize, HashMap<String, usize>>,
        mean_genotypes: f32,
        tid: i32,
        total_indels: usize,
        target_name: Vec<u8>,
        target_len: usize,
        variations_per_base: f32,
        coverage: f32,
        variance: f32,
        observed_contig_length: u32,
        num_covered_bases: i32,
        contig_end_exclusion: u32,
        min_fraction_covered_bases: f32,
        min: f32,
        max: f32,
    }
}

impl PileupStats {
    pub fn new_contig_stats(min: f32, max: f32, min_fraction_covered_bases: f32,
                            contig_end_exclusion: u32) -> PileupStats {
        PileupStats::PileupContigStats {
            nucfrequency: vec!(),
            variants_in_reads: HashMap::new(),
            variant_abundances: Vec::new(),
            depth: vec!(),
            indels: vec!(),
            genotypes_per_position: HashMap::new(),
            mean_genotypes: 0.0,
            tid: 0,
            total_indels: 0,
            target_name: vec!(),
            target_len: 0,
            variations_per_base: 0.00,
            coverage: 0.00,
            variance: 0.00,
            observed_contig_length: 0,
            num_covered_bases: 0,
            contig_end_exclusion: contig_end_exclusion,
            min_fraction_covered_bases: min_fraction_covered_bases,
            min: min,
            max: max,
        }
    }
}

pub trait PileupFunctions {
    fn setup(&mut self);

    fn add_contig(&mut self,
                  nuc_freq: Vec<HashMap<char, HashSet<i32>>>,
                  read_depth: Vec<usize>,
                  indels_positions: Vec<HashMap<String, HashSet<i32>>>,
                  tid: i32,
                  total_indels_in_contig: usize,
                  contig_name: Vec<u8>,
                  contig_len: usize);

    fn calc_variants(&mut self,
                     min_variant_depth: usize);

    fn generate_variant_contig(&mut self,
                               original_contig: Vec<u8>,
                               consensus_genome: std::fs::File);

    fn generate_genotypes(&mut self);

    fn calc_coverage(&mut self, total_mismatches: u32, method: &str) -> f32;

    fn calc_gene_mutations(&mut self,
                           gff_map: &HashMap<String, Vec<bio::io::gff::Record>>,
                           ref_sequence: &Vec<u8>,
                           codon_table: &CodonTable);

    fn print_variants(&mut self, ref_sequence: &Vec<u8>, sample_idx: i32);
}

impl PileupFunctions for PileupStats {
    fn setup(&mut self) {
        match self {
            PileupStats::PileupContigStats {
                ref mut nucfrequency,
                ref mut variants_in_reads,
                ref mut variant_abundances,
                ref mut depth,
                ref mut indels,
                ref mut tid,
                ref mut total_indels,
                ref mut target_name,
                ref mut target_len,
                ref mut variations_per_base,
                ref mut coverage,
                ref mut num_covered_bases,
                ..
            } => {
                *nucfrequency = vec!();
                *variants_in_reads = HashMap::new();
                *variant_abundances = Vec::new();
                *depth = vec!();
                *indels = vec!();
                *tid = 0;
                *total_indels = 0;
                *target_name = vec!();
                *target_len = 0;
                *variations_per_base = 0.00;
                *coverage = 0.00;
                *num_covered_bases = 0;
            }
        }
    }

    fn add_contig(&mut self, nuc_freq: Vec<HashMap<char, HashSet<i32>>>,
                  read_depth: Vec<usize>,
                  indel_positions: Vec<HashMap<String, HashSet<i32>>>,
                  target_id: i32,
                  total_indels_in_contig: usize,
                  contig_name: Vec<u8>,
                  contig_len: usize) {
        match self {
            PileupStats::PileupContigStats {
                ref mut nucfrequency,
                ref mut depth,
                ref mut indels,
                ref mut tid,
                ref mut total_indels,
                ref mut target_name,
                ref mut target_len,
                ..
            } => {
                *nucfrequency = nuc_freq;
                *depth = read_depth;
                *indels = indel_positions;
                *tid = target_id;
                *total_indels = total_indels_in_contig;
                *target_name = contig_name;
                *target_len = contig_len;

            }
        }
    }

    fn calc_variants(&mut self, min_variant_depth: usize){
        match self {
            PileupStats::PileupContigStats {
                ref mut nucfrequency,
                ref mut variants_in_reads,
                ref mut variant_abundances,
                depth,
                ref mut indels,
                target_len,
                ref mut variations_per_base,
                ref mut coverage,
                tid,
                ..
            } => {
                let variants = Arc::new(Mutex::new(vec![HashMap::new(); *target_len])); // The relative abundance of each variant
                let read_variants = Arc::new(Mutex::new(HashMap::new())); // The reads with variants and their positions
                let variant_count = Arc::new(Mutex::new(0));
                let indels_backup = Arc::new(Mutex::new(indels.clone()));
                let nucfrequency_backup = Arc::new(Mutex::new(nucfrequency.clone()));

                // for each location calculate if there is a variant based on read depth
                // Uses rayon multithreading
                depth.into_par_iter().enumerate().for_each(|(i, d)| {
                    let read_variants = Arc::clone(&read_variants);
                    let variant_count = Arc::clone(&variant_count);
                    let mut rel_abundance = HashMap::new();
                    if *coverage * 0.75 <= *d as f32 && *d as f32 <= *coverage * 1.25 {
//                        if d >= &mut min_variant_depth.clone() {
                            if nucfrequency[i].len() > 0 {
                                for (base, read_ids) in nucfrequency[i].iter() {
                                    let count = read_ids.len();
                                    if count >= min_variant_depth {
                                        rel_abundance.insert(base.to_string(), count as f32 / *d as f32);
                                        for read in read_ids {
                                            let mut read_variants
                                                = read_variants.lock().unwrap();
                                            let read_vec = read_variants
                                                .entry(read.clone())
                                                .or_insert(BTreeMap::new());
                                            read_vec.insert(i as i32, base.to_string());
                                        }
                                        let mut variant_count = variant_count.lock().unwrap();
                                        *variant_count += 1;
                                    } else {
                                        let mut nucfrequency_backup
                                            = nucfrequency_backup.lock().unwrap();
                                        nucfrequency_backup[i].remove(base);
                                    }
                                }
                            };
                            if indels[i].len() > 0 {
                                for (indel, read_ids) in indels[i].iter() {
                                    let count = read_ids.len();
                                    if count >= min_variant_depth {
                                        rel_abundance.insert(indel.clone(), count as f32 / *d as f32);
                                        for read in read_ids {
                                            let mut read_variants
                                                = read_variants.lock().unwrap();

                                            let read_vec = read_variants
                                                .entry(read.clone())
                                                .or_insert(BTreeMap::new());
                                            read_vec.insert(i as i32, indel.clone());
                                        }
                                        let mut variant_count = variant_count.lock().unwrap();
                                        *variant_count += 1;
                                    } else {
                                        let mut indels_backup
                                            = indels_backup.lock().unwrap();
                                        indels_backup[i].remove(indel);
                                    }
                                }
                            }
//                        }
                    }

                    if rel_abundance.len() > 0 {
                        let mut variants = variants.lock().unwrap();
                        variants[i] = rel_abundance;
                    }

                });

                let read_variants = read_variants.lock().unwrap();
                *variants_in_reads = read_variants.clone();
                let variants = variants.lock().unwrap();
                *variant_abundances = variants.clone();
                let variant_count = variant_count.lock().unwrap();
                debug!("Total variants for {}: {:?}", tid, variant_count);
                *variations_per_base = (*variant_count) as f32/target_len.clone() as f32;
                let nucfrequency_backup = nucfrequency_backup.lock().unwrap();
                *nucfrequency = nucfrequency_backup.clone();
                let indels_backup = indels_backup.lock().unwrap();
                *indels = indels_backup.clone();
            }
        }
    }

    fn generate_variant_contig(&mut self,
                               original_contig: Vec<u8>,
                               mut consensus_genome: std::fs::File){
        match self {
            PileupStats::PileupContigStats {
                ref mut variant_abundances,
                ..
            } => {
                let mut contig = String::new();

                let mut skip_n = 0;
                let mut skip_cnt = 0;
                // Generate the consensus genome by checking each variant
                // Variant has to be in more than 0.5 of population
                for (pos, base) in original_contig.iter().enumerate() {
                    if skip_cnt < skip_n {
                        skip_cnt += 1;
                    } else {
                        let mut max_var = "";
                        let mut max_abund = 0.0;
                        skip_n = 0;
                        skip_cnt = 0;
                        if variant_abundances[pos].len() > 0 {
                            let hash = &variant_abundances[pos];
                            for (var, abundance) in hash.iter() {
                                if abundance > &max_abund {
                                    max_var = var;
                                    max_abund = *abundance;
                                }
                            }
                            if max_abund >= 0.5{
                                if max_var.contains("N") {
                                    skip_n = max_var.len() - 1;
                                    skip_cnt = 0;
                                } else {
                                    contig = contig + max_var;
                                }
                            } else {
                                contig = contig + str::from_utf8(&[*base]).unwrap();
                            }
                        } else {
                            contig = contig + str::from_utf8(&[*base]).unwrap();
                        }
                    }
                };
                contig = contig + "\n";
                match consensus_genome.write_all(contig.as_bytes()) {
                    Ok(consensus_genome) => consensus_genome,
                    Err(e) => {
                        println!("Cannot write to file {:?}", e);
                        std::process::exit(1)}
                };
            }
        }
    }

    fn generate_genotypes(&mut self) {
        match self {
            PileupStats::PileupContigStats {
                ref mut variant_abundances,
                ref mut nucfrequency,
                ref mut indels,
                ref mut variants_in_reads,
                ref mut genotypes_per_position,
                ref mut mean_genotypes,
                tid,
                target_len,
                variations_per_base,
                coverage,
                ..
            } => {
                let genotypes =
                    Arc::new(Mutex::new(HashMap::new()));
                let variant_count =
                    Arc::new(Mutex::new(0));
                let total_genotype_count =
                    Arc::new(Mutex::new(0));

                debug!("starting genotyping of tid {}, of length {}, and var per b {} at {} times coverage",
                        tid, target_len, variations_per_base, coverage);

                variant_abundances.par_iter().enumerate().for_each(|(position, variants)| {
                    // For each variant we calculate the minimum number of genotypes possible
                    // based on variants in reads mapping to this variant location
                    if variants.len() > 0 {
                        let mut genotype_record;

                        let genotypes = Arc::clone(&genotypes);
                        let variant_count = Arc::clone(&variant_count);
                        let total_genotype_count = Arc::clone(&total_genotype_count);

                        let mut genotype_pos = HashMap::new();

                        for (var, _abundance) in variants.iter() {
                            let genotype_count = genotype_pos.entry(var.to_string())
                                .or_insert(0);

                            let mut genotype_vec = Vec::new();

                            genotype_record = Genotype {
                                read_ids: HashSet::new(),
                                base_positions: Vec::new(),
                                start_var_pos: position,
                                ordered_variants: HashMap::new(),
                            };
                            let mut read_ids = HashSet::new();
                            if indels[position].contains_key(var) {
                                read_ids =
                                    match indels[position].get(var) {
                                        Some(ids) => ids.clone(),
                                        None => {
                                            println!("Variant not in indel hash");
                                            std::process::exit(1)
                                        },
                                    };
                            } else if nucfrequency[position].contains_key(
                                &(var.clone().into_bytes()[0] as char)) {
                                read_ids =
                                    match nucfrequency[position]
                                        .get(&(var.clone().into_bytes()[0] as char)) {
                                        Some(ids) => ids.clone(),
                                        None => {
                                            println!("Variant not in frequency Hash");
                                            std::process::exit(1)
                                        },
                                    };
                            }
                            let mut left_most_variants: Vec<i32> = Vec::new();
                            let read_vec = read_ids.into_iter().collect::<Vec<i32>>();

                            for read_id in read_vec.iter() {
                                let position_map = match variants_in_reads.get(read_id) {
                                    Some(positions) => positions,
                                    None => {
                                        debug!("read id not recorded in variant map {}, {}", var, read_id);
                                        break
                                    },
                                };
                                left_most_variants.push(
                                    *position_map.keys().cloned().collect::<Vec<i32>>().iter().min().unwrap());
                            }
                            // Generate the permutation of read id indices that create this list ordering
                            let permuted = permutation::sort(&left_most_variants[..]);
                            // Order the read vec by this permutation
                            let read_vec_sorted = permuted.apply_slice(&read_vec[..]);

                            // Loop through reads based on their left most variant position
                            for read_id in read_vec_sorted.iter() {
                                let position_map = match variants_in_reads.get(read_id) {
                                    Some(positions) => positions,
                                    None => {
                                        debug!("read id not recorded in variant map {}, {}", var, read_id);
                                        break
                                    },
                                };

                                if genotype_vec.len() == 0 {
                                    // No genotype observed yet, so create one
                                    genotype_record.read_ids.insert(*read_id);
                                    for (pos, variant) in position_map.iter() {
                                        genotype_record.base_positions.push(pos.clone());
                                        genotype_record.ordered_variants.insert(pos.clone(), variant.to_string());
                                    }
                                    genotype_record.base_positions.sort();
                                    genotype_vec.push(genotype_record.clone());
                                } else {
//                                    let position_map_variants: Vec<String> = position_map.values().cloned().collect();
                                    let position_map_positions: Vec<i32> = position_map.keys().cloned().collect();
                                    let position_set = position_map.keys().cloned().collect::<HashSet<i32>>();

                                    let mut new_genotype = false;

                                    for genotype in genotype_vec.iter_mut() {

                                        // Create HashSets of variant positions to use intersection
                                        let genotype_position_set =
                                            genotype.base_positions.iter().cloned().collect::<HashSet<i32>>();

                                        let diff: Vec<i32> = genotype_position_set
                                            .symmetric_difference(&position_set).cloned().collect();

                                        if diff.len() > 0 {
                                            // Positional difference found
                                            // Check if new genotype
                                            for pos in diff.iter() {
                                                if (genotype.base_positions.iter().min() < Some(pos))
                                                    && (Some(pos) < genotype.base_positions.iter().max()) {
                                                    // possible new genotype detected
                                                    genotype_record.read_ids = HashSet::new();
                                                    genotype_record.read_ids.insert(*read_id);
                                                    for (pos, variant) in position_map.iter() {
                                                        genotype_record.base_positions.push(pos.clone());
                                                        genotype_record.ordered_variants.insert(pos.clone(), variant.to_string());
                                                    }
                                                    genotype_record.base_positions.sort();
//                                                    new_genotype = true;
                                                    break
                                                }
                                            }
                                            if (genotype.base_positions.iter().min() == diff.iter().min())
                                                || (diff.iter().max() > genotype.base_positions.iter().max()) {
                                                // check variants against stored variants for a genotype
                                                for (check_pos, check_var) in position_map.iter() {
                                                    if genotype.ordered_variants.contains_key(&check_pos) {
                                                        let current_var = match genotype
                                                            .ordered_variants
                                                            .get(&check_pos) {
                                                            Some(var) => var,
                                                            None => {
                                                                println!("Position not recorded in variant map");
                                                                std::process::exit(1)
                                                            }
                                                        };
                                                        if current_var != check_var {
                                                            //Then this is a new genotype
                                                            new_genotype = true;
                                                        }
                                                    }
                                                }
                                                if new_genotype {
                                                    // possible new genotype detected
                                                    genotype_record.read_ids = HashSet::new();
                                                    genotype_record.read_ids.insert(*read_id);
                                                    for (pos, variant) in position_map.iter() {
                                                        genotype_record.base_positions.push(pos.clone());
                                                        genotype_record.ordered_variants.insert(pos.clone(), variant.to_string());
                                                    }
                                                    genotype_record.base_positions.sort();
                                                } else {
                                                    // Extension of previous genotype
                                                    genotype.read_ids.insert(*read_id);
                                                    for base_position in position_map_positions.iter() {
                                                        if !(genotype.base_positions.contains(&base_position)) {
                                                            genotype.base_positions.push(*base_position);
                                                        }
                                                    };
                                                    genotype.base_positions.sort();
                                                    for (new_position, new_variant) in position_map.iter() {
                                                        if !(genotype.ordered_variants.contains_key(&new_position)) {
                                                            genotype.ordered_variants
                                                                .insert(new_position.clone(), new_variant.clone());
                                                        }
                                                    };

                                                    // reset genotype_record
                                                    genotype_record = Genotype {
                                                        read_ids: HashSet::new(),
                                                        base_positions: Vec::new(),
                                                        start_var_pos: position,
                                                        ordered_variants: HashMap::new(),
                                                    };
                                                    genotype.read_ids.insert(*read_id);
                                                    break
                                                }
                                            }
                                        } else {
                                            // check variants against stored variants for a genotype
                                            for (check_pos, check_var) in position_map.iter() {
                                                if genotype.ordered_variants.contains_key(&check_pos) {
                                                    let current_var = match genotype
                                                        .ordered_variants
                                                        .get(&check_pos) {
                                                        Some(var) => var,
                                                        None => {
                                                            println!("Position not recorded in variant map");
                                                            std::process::exit(1)
                                                        }
                                                    };
                                                    if current_var != check_var {
                                                        //Then this is a new genotype
                                                        new_genotype = true;
                                                    }
                                                }
                                            }
                                            if new_genotype {
                                                // possible new genotype detected
                                                genotype_record.read_ids = HashSet::new();
                                                genotype_record.read_ids.insert(*read_id);
                                                for (pos, variant) in position_map.iter() {
                                                    genotype_record.base_positions.push(pos.clone());
                                                    genotype_record.ordered_variants.insert(pos.clone(), variant.to_string());
                                                }
                                                genotype_record.base_positions.sort();
                                            } else {
                                                // No difference with a previous genotype, reset current
                                                genotype_record = Genotype {
                                                    read_ids: HashSet::new(),
                                                    base_positions: Vec::new(),
                                                    start_var_pos: position,
                                                    ordered_variants: HashMap::new(),
                                                };
                                                genotype.read_ids.insert(*read_id);
                                                break
                                            }
                                        }
                                    }
                                    if genotype_record.base_positions.len() > 0 {
                                        // New genotype detected
                                        genotype_vec.push(genotype_record);

                                        genotype_record = Genotype {
                                            read_ids: HashSet::new(),
                                            base_positions: Vec::new(),
                                            start_var_pos: position,
                                            ordered_variants: HashMap::new(),
                                        };
                                    }
                                }
                            }

                            *genotype_count += genotype_vec.len();
                            let mut total_genotype_count = total_genotype_count.lock().unwrap();
                            *total_genotype_count += genotype_vec.len();
                            let mut variant_count = variant_count.lock().unwrap();
                            *variant_count += 1;
                        }
                        let mut genotypes = genotypes.lock().unwrap();
                        genotypes.insert(position, genotype_pos);
                    }
                });
                //Calc the mean number of genotypes per variant
                let variant_count = variant_count.lock().unwrap();
                let total_genotype_count = total_genotype_count.lock().unwrap();
                if *variant_count > 0 {
                    *mean_genotypes = *total_genotype_count as f32 / *variant_count as f32;
                } else {
                    *mean_genotypes = 0.0 as f32;
                }
                *genotypes_per_position = genotypes.lock().unwrap().clone();
            }
        }
    }

    fn calc_coverage(&mut self, total_mismatches: u32, method: &str) -> f32 {
        match self {
            PileupStats::PileupContigStats {
                ref mut depth,
                target_len,
                ref mut coverage,
                ref mut variance,
                observed_contig_length,
                num_covered_bases,
                contig_end_exclusion,
                min_fraction_covered_bases,
                min,
                max,
                ..

            } => {
                let len1 = target_len;
                match *contig_end_exclusion * 2 < *len1 as u32 {
                    true => {
                        *observed_contig_length += *len1 as u32 - 2 * *contig_end_exclusion
                    },
                    false => {
                        debug!("Contig too short - less than twice the contig-end-exclusion");
                    }
                }

                debug!("Total observed length now {}", *observed_contig_length);
                let mut counts: Vec<usize> = vec!();
                let start_from = *contig_end_exclusion as usize;
                let end_at = *len1 - *contig_end_exclusion as usize - 1;
                let mut cumulative_sum;
                let mut total_count = 0;
                for (i, current) in depth.iter().enumerate() {
                    cumulative_sum = *current;
                    if i >= start_from && i <= end_at {
                        if cumulative_sum > 0 {
                            *num_covered_bases += 1
                        }
                        if counts.len() <= cumulative_sum {
                            (counts).resize(cumulative_sum + 1, 0);
                        }
                        (counts)[cumulative_sum] += 1;
                        total_count += cumulative_sum;
                    }
                }

                let total_bases = *observed_contig_length;

                let mut answer;
                match method {
                    "trimmed_mean" => {

                        answer = match total_bases {
                            0 => {
                                *variance = 0.0;
                                0.0
                            },
                            _ => {
                                if (*num_covered_bases as f32 / total_bases as f32) < *min_fraction_covered_bases {
                                    *variance = 0.0;
                                    0.0
                                } else {
                                    let min_index: usize = (*min * total_bases as f32).floor() as usize;
                                    let max_index: usize = (*max * total_bases as f32).ceil() as usize;
                                    if *num_covered_bases == 0 { return 0.0; }
//                            counts[0] += 0;

                                    let mut num_accounted_for: usize = 0;
                                    let mut total: usize = 0;
                                    let mut started = false;
                                    let mut i = 0;

                                    let mut k = 0;
                                    // Ensure K is within the range of coverages - take the
                                    // lowest coverage.
                                    while counts[k] == 0 {
                                        k += 1;
                                    }
                                    let mut ex = 0;
                                    let mut ex2 = 0;

                                    for (x, num_covered) in counts.iter().enumerate() {
                                        num_accounted_for += num_covered.clone() as usize;

                                        if num_covered > &0 {
                                            let nc = *num_covered as usize;
                                            ex += (x - k) * nc;
                                            ex2 += (x - k) * (x - k) * nc;
                                        }
                                        if num_accounted_for >= min_index {
                                            if started {
                                                if num_accounted_for > max_index {
                                                    let num_excess = num_accounted_for - *num_covered as usize;
                                                    let num_wanted = match max_index >= num_excess {
                                                        true => max_index - num_excess + 1,
                                                        false => 0
                                                    };
                                                    total += num_wanted * i;
                                                    break;
                                                } else {
                                                    total += *num_covered as usize * i;
                                                }
                                            } else {
                                                if num_accounted_for > max_index {
                                                    // all coverages are the same in the trimmed set
                                                    total = (max_index - min_index + 1) * i;
                                                    started = true
                                                } else if num_accounted_for < min_index {
                                                    debug!("too few on first")
                                                } else {
                                                    let num_wanted = num_accounted_for - min_index + 1;
                                                    total = num_wanted * i;
                                                    started = true;
                                                }
                                            }
                                        }
                                        i += 1;
                                    }
                                    // Return sample variance not population variance since
                                    // almost all MAGs are incomplete.
                                    *variance =
                                        (ex2 as f32 - (ex * ex) as f32 / total_bases as f32) / (total_bases - 1) as f32;
                                    total as f32 / (max_index - min_index) as f32
                                }
                            }
                        };
                        *coverage = answer.clone();
                        return answer
                    },
                    _ => {
                        let exclude_mismatches = false;

                        answer = match total_bases {
                            0 => {
                                *variance = 0.0;
                                0.0
                            },
                            _ => {
                                if (*num_covered_bases as f32 / total_bases as f32) < *min_fraction_covered_bases {
                                    *variance = 0.0;
                                    0.0
                                } else {
                                    // Calculate variance using the shifted method
                                    let mut k = 0;
                                    // Ensure K is within the range of coverages - take the
                                    // lowest coverage.
                                    while counts[k] == 0 {
                                        k += 1;
                                    }
                                    let mut ex = 0;
                                    let mut ex2 = 0;
                                    for (x, num_covered) in counts.iter().enumerate() {
                                        if *num_covered == 0 { continue }
                                        let nc = *num_covered as usize;
                                        ex += (x-k) * nc;
                                        ex2 += (x-k)*(x-k) * nc;
                                    }


                                    // Return sample variance not population variance since
                                    // almost all MAGs are incomplete.
                                    *variance =
                                        (ex2 as f32 - (ex * ex) as f32 / total_bases as f32) / (total_bases - 1) as f32;

                                    (match exclude_mismatches {
                                        true => (total_count - total_mismatches as usize) as f32,
                                        false => total_count as f32
                                    }) / total_bases as f32
                                }
                            }
                        };
                        *coverage = answer.clone();
                        return answer
                    },
                }
            }
        }
    }

    fn calc_gene_mutations(&mut self,
                           gff_map: &HashMap<String, Vec<bio::io::gff::Record>>,
                           ref_sequence: &Vec<u8>,
                           codon_table: &CodonTable) {
        match self {
            PileupStats::PileupContigStats {
                indels,
                variant_abundances,
                tid,
                target_name,
                depth,
                ..
            } => {
                let contig_name = String::from_utf8(target_name.clone())
                    .expect("Cannot create string from target_name");
                let placeholder = Vec::new();
                let gff_records = match gff_map.get(&contig_name){
                    Some(records) => records,
                    None => &placeholder,
                };
                gff_records.par_iter().for_each(|gene| {
                    let dnds = codon_table.find_mutations(gene, variant_abundances, ref_sequence, depth);
                    println!("for gene {} {}-{}, dN/dS is {}", gene.seqname(), gene.start(), gene.end(), dnds);
                })
            }
        }

    }

    fn print_variants(&mut self, ref_sequence: &Vec<u8>, sample_idx: i32){
        match self {
            PileupStats::PileupContigStats {
                indels,
                variant_abundances,
                depth,
                tid,
                genotypes_per_position,
                ..

            } => {
                variant_abundances.par_iter().enumerate().for_each(|(position, hash)|{
                    // loop through each position that has variants
//                    let position = *position as usize;
                    let d = depth[position];

                    for (var, abundance) in hash.iter() {
                        // for each variant at a location
                        if indels[position].contains_key(var) {
                            // How does this print N for insertions?
                            if var.to_owned().contains("N"){
                                print!("{}\t{}\t{}\t{}\t{}\t{}\t", tid, position,
                                       var,
                                       str::from_utf8(
                                           &ref_sequence[position..position
                                               + var.len() as usize]).unwrap(),
                                       abundance, d);

                            } else {
                                print!("{}\t{}\t{}\t{}\t{}\t{}\t", tid, position-1,
                                       str::from_utf8(
                                           &[ref_sequence[position-1]]).unwrap().to_owned() + var,
                                       str::from_utf8(
                                           &[ref_sequence[position-1]]).unwrap(),
                                       abundance, d);
                            }

                            // Print number of genotypes associated with that position and variant
                            match genotypes_per_position.get(&position) {
                                Some(gtype_hash) => {
                                    match gtype_hash.get(&var.to_string()) {
                                        Some(gtype_count) => {
                                            print!("{}\t", gtype_count);
                                        },
                                        None => {
                                            print!("0\t");
                                        }
                                    }
                                },
                                None => {
                                    print!("0\t");
                                },
                            };
                            println!{"{}", sample_idx};

                        } else if var.len() == 1{
                            print!("{}\t{}\t{}\t{}\t{}\t{}\t", tid, position,
                                   var,
                                   ref_sequence[position] as char,
                                   abundance, d);

                            // Print number of genotypes associated with that position and variant
                            match genotypes_per_position.get(&position) {
                                Some(gtype_hash) => {
                                    match gtype_hash.get(&var.to_string()) {
                                        Some(gtype_count) => {
                                            print!("{}\t", gtype_count);
                                        },
                                        None => {
                                            print!("0\t");
                                        }
                                    }
                                },
                                None => {
                                    print!("0\t");
                                },
                            };
                            println!{"{}", sample_idx};
                        }
                    }
                });
            }
        }
    }
}