#![allow(unused)]
use std::{collections::HashMap, error::Error};

use num_bigint::BigInt;

#[derive(Debug, Clone)]
struct CompressDataDescription {
    start_byte: usize,
    amount_bytes: usize,
    method: String,
}

#[derive(Debug, Clone, Default)]
struct CompressDataPower {
    decompressed_size: usize,
    compressed_size: usize,
}

impl CompressDataPower {
    fn range(&self) -> usize {
        self.decompressed_size - self.compressed_size
    }

    fn add(&mut self, other: &CompressDataPower) {
        self.decompressed_size += other.decompressed_size;
        self.compressed_size += other.compressed_size;
    }
}

#[derive(Debug, Clone)]
struct CompressData {
    power: CompressDataPower,
    description: Vec<CompressDataDescription>,
}

#[derive(Debug, Clone)]
struct Calldata {
    data: String,
    contract: String,
    bytes_info: Vec<ByteInfo>,
    dict: Vec<String>,
    lookup: HashMap<Vec<u8>, usize>,
}

#[derive(Debug, Clone)]
struct ByteInfo {
    index: usize,
    zero_compress: CompressDataPower,
    copy_compress: CompressDataPower,
    storage_compress: Vec<CompressDataPower>,
}

impl Calldata {
    pub fn new(data: &str, decompressor_extension: &str) -> Result<Self, &'static str> {
        let data = data.strip_prefix("0x").unwrap();
        let mut data_trim_0 = data.trim_start_matches('0').to_lowercase();
        if data_trim_0.is_empty() {
            data_trim_0 = "0".to_string();
        }
        let data_bigint = u64::from_str_radix(data, 16).unwrap();
        let data_bigint_str = format!("{:x}", data_bigint);
        if data_bigint_str != data_trim_0 {
            panic!("The data is not hexadecimal");
        }
        if data.len() % 2 != 0 {
            panic!("The data is not hexadecimal");
        }

        Ok(Calldata {
            data: data.to_string(),
            contract: decompressor_extension.to_string(),
            bytes_info: Vec::new(),
            dict: Vec::new(),
            lookup: HashMap::new(),
        })
    }

    pub fn analyse(&mut self) -> Vec<ByteInfo> {
        self.bytes_info = vec![];
        for i in (0..self.data.len()).step_by(2) {
            self.bytes_info.push(ByteInfo {
                index: i / 2,
                zero_compress: self.check_zeros_case(i / 2),
                copy_compress: self.check_copy_case_with_zeros(i / 2),
                storage_compress: self.check_storage_case(i / 2).unwrap(),
            });
        }
        self.bytes_info.clone()
    }

    fn create_desc(
        &self,
        array_desc: &Vec<CompressDataDescription>,
        amount_bytes: usize,
        method: &str,
    ) -> CompressDataDescription {
        let start_byte: usize = if !array_desc.is_empty() {
            let prev_desc_index = array_desc.len() - 1;
            array_desc[prev_desc_index].start_byte + array_desc[prev_desc_index].amount_bytes
        } else {
            0
        };
        CompressDataDescription {
            start_byte,
            amount_bytes,
            method: method.to_string(),
        }
    }

    fn add_just_copy_compress(
        &self,
        mut result_compress: CompressData,
        amount: usize,
    ) -> CompressData {
        if amount != 0 {
            result_compress.power.add(&CompressDataPower {
                decompressed_size: amount,
                compressed_size: 1 + amount,
            });
            result_compress.description.push(self.create_desc(
                &result_compress.description,
                amount,
                "01",
            ));
        }
        result_compress
    }

    #[allow(clippy::int_plus_one)]
    fn compress_part(&self, from_byte: usize, to_byte: usize) -> CompressData {
        let mut part_compress = CompressData {
            power: CompressDataPower {
                decompressed_size: 0,
                compressed_size: 0,
            },
            description: vec![],
        };
        let mut just_copy_amount: usize = 0;

        for mut i in from_byte..to_byte {
            if (self.bytes_info[i].zero_compress.decompressed_size >= to_byte - i + 1) {
                part_compress = self.add_just_copy_compress(part_compress, just_copy_amount);
                part_compress.power.add(&CompressDataPower {
                    decompressed_size: to_byte - from_byte + 1,
                    compressed_size: 1,
                });
                part_compress.description.push(CompressDataDescription {
                    start_byte: i,
                    amount_bytes: to_byte - i + 1,
                    method: "00".to_string(),
                });
                return part_compress;
            }

            let mut zero_bytes_amount = 0;
            let mut is_padding_with_copy = false;
            let mut need_just_copy_amount = true;

            if self.bytes_info[i].zero_compress.decompressed_size != 0 {
                if self.bytes_info[i].copy_compress.decompressed_size >= to_byte - i + 1
                    || self.bytes_info[i].zero_compress.range()
                        > self.bytes_info[i].copy_compress.range()
                {
                    zero_bytes_amount = self.bytes_info[i].zero_compress.decompressed_size;
                } else {
                    is_padding_with_copy = true;
                }
            }
            let mut is_storage_compress_used: bool = false;
            let is_zero_compress: bool = zero_bytes_amount > 0;
            for j in 0..self.bytes_info[i].storage_compress.len() {
                if self.bytes_info[i].storage_compress[j].decompressed_size <= to_byte - i + 1 {
                    let is_storage_range_more_than_copy_compress = self.bytes_info[i]
                        .storage_compress[j]
                        .range()
                        > self.bytes_info[i].copy_compress.range();

                    if !is_zero_compress
                        && !is_storage_range_more_than_copy_compress
                        && !is_padding_with_copy
                    {
                        continue;
                    }

                    part_compress = self.add_just_copy_compress(part_compress, just_copy_amount);

                    if is_zero_compress {
                        if self.bytes_info[i].storage_compress[j].range()
                            > self.bytes_info[i].zero_compress.range()
                        {
                            part_compress
                                .power
                                .add(&self.bytes_info[i].clone().storage_compress[j].clone());
                            part_compress.description.push(self.create_desc(
                                &part_compress.description,
                                self.bytes_info[i].storage_compress[j].decompressed_size,
                                if self.bytes_info[i].storage_compress[j].compressed_size == 2 {
                                    "10"
                                } else {
                                    "11"
                                },
                            ));
                            i += self.bytes_info[i].storage_compress[j].decompressed_size;
                        } else {
                            part_compress
                                .power
                                .add(&self.bytes_info[i].clone().zero_compress.clone());
                            part_compress.description.push(self.create_desc(
                                &part_compress.description,
                                zero_bytes_amount,
                                "00",
                            ));
                            i += zero_bytes_amount;
                        }
                    } else if is_storage_range_more_than_copy_compress {
                        part_compress
                            .power
                            .add(&self.bytes_info[i].clone().storage_compress[j].clone());
                        part_compress.description.push(self.create_desc(
                            &part_compress.description,
                            self.bytes_info[i].storage_compress[j].decompressed_size,
                            if self.bytes_info[i].storage_compress[j].compressed_size == 2 {
                                "10"
                            } else {
                                "11"
                            },
                        ));
                        i += self.bytes_info[i].storage_compress[j].decompressed_size;
                    } else if is_padding_with_copy {
                        part_compress
                            .power
                            .add(&self.bytes_info[i].clone().copy_compress);
                        part_compress.description.push(self.create_desc(
                            &part_compress.description,
                            self.bytes_info[i].copy_compress.decompressed_size,
                            "01",
                        ));
                        i += self.bytes_info[i].copy_compress.decompressed_size;
                    }

                    just_copy_amount = 0;
                    need_just_copy_amount = false;
                    is_storage_compress_used = true;
                    break;
                }
            }

            if !is_storage_compress_used {
                if is_zero_compress || is_padding_with_copy {
                    part_compress = self.add_just_copy_compress(part_compress, just_copy_amount);
                }

                if is_zero_compress {
                    part_compress
                        .power
                        .add(&self.bytes_info[i].clone().zero_compress);
                    part_compress.description.push(self.create_desc(
                        &part_compress.description,
                        zero_bytes_amount,
                        "00",
                    ));
                    i += zero_bytes_amount;
                } else if is_padding_with_copy {
                    part_compress
                        .power
                        .add(&self.bytes_info[i].clone().copy_compress);
                    part_compress.description.push(self.create_desc(
                        &part_compress.description,
                        self.bytes_info[i].copy_compress.decompressed_size,
                        "01",
                    ));
                    i += self.bytes_info[i].copy_compress.decompressed_size;
                }

                if is_zero_compress || is_padding_with_copy {
                    just_copy_amount = 0;
                    need_just_copy_amount = false;
                }
            }
            if need_just_copy_amount {
                let new_just_copy_amount = std::cmp::min(
                    self.bytes_info[i].copy_compress.decompressed_size,
                    to_byte - i + 1,
                );
                just_copy_amount += new_just_copy_amount;
                if just_copy_amount > 32 {
                    part_compress = self.add_just_copy_compress(part_compress, 32);
                    just_copy_amount -= 32;
                }
                i += new_just_copy_amount;
            }
        }

        part_compress = self.add_just_copy_compress(part_compress, just_copy_amount);

        part_compress
    }

    fn scale_fraction(&self, fraction: &str) -> String {
        if fraction.len() % 2 != 0 {
            format!("{}{}", "0", fraction)
        } else {
            fraction.to_string()
        }
    }

    fn zip(&self, instractions: Vec<CompressDataDescription>) -> String {
        let mut result = "0x".to_owned();
        let bb = [32, 20, 4, 31];
        let mut index = 0;
        for instraction in instractions.iter() {
            match instraction.method.as_str() {
                "00" => {
                    result.push_str(&self.scale_fraction(
                        &BigInt::from(instraction.amount_bytes - 1).to_str_radix(16),
                    ));
                }
                "01" => {
                    let copy_bytes =
                        self.get_bytes(instraction.start_byte, instraction.amount_bytes);
                    let mut non_zero_byte_index = 0;
                    for j in 0..instraction.amount_bytes {
                        if copy_bytes.get(j * 2..j * 2 + 2) != Some("00".as_bytes()) {
                            non_zero_byte_index = j;
                            break;
                        }
                    }
                    result.push_str(
                        &self.scale_fraction(
                            &BigInt::from(
                                (instraction.amount_bytes - non_zero_byte_index - 1)
                                    + 64
                                    + if (non_zero_byte_index == 0) { 0 } else { 32 },
                            )
                            .to_str_radix(16),
                        ),
                    );
                    result.push_str(&hex::encode(&self.get_bytes(
                        instraction.start_byte + non_zero_byte_index,
                        instraction.amount_bytes - non_zero_byte_index,
                    )));
                }
                "10" => {
                    index = *self
                        .lookup
                        .get(&self.get_bytes(instraction.start_byte, instraction.amount_bytes))
                        .unwrap_or(&0);
                    result.push_str(&hex::encode(
                        &self.scale_fraction(
                            &BigInt::from(
                                index
                                    + 2_i64.pow(15) as usize
                                    + (bb
                                        .iter()
                                        .position(|&r| r == instraction.amount_bytes)
                                        .unwrap()
                                        * 2_i64.pow(12) as usize),
                            )
                            .to_str_radix(16),
                        ),
                    ));
                }
                "11" => {
                    index = *self
                        .lookup
                        .get(&self.get_bytes(instraction.start_byte, instraction.amount_bytes))
                        .unwrap_or(&0);
                    result.push_str(&hex::encode(
                        &self.scale_fraction(
                            &BigInt::from(
                                index
                                    + 3 * 2_i64.pow(22) as usize
                                    + (bb
                                        .iter()
                                        .position(|&r| r == instraction.amount_bytes)
                                        .unwrap()
                                        * 2_i64.pow(20) as usize),
                            )
                            .to_str_radix(16),
                        ),
                    ));
                }
                _ => {
                    panic!("Unsupported method: {}", instraction.method);
                }
            }
        }
        result
    }

    fn compress(&mut self) -> CompressResult {
        self.analyse();

        let mut best_compress_for_first_n_bytes: Vec<CompressData> = Vec::new();

        if self.bytes_info[0].zero_compress.decompressed_size != 0 {
            best_compress_for_first_n_bytes.push(CompressData {
                power: CompressDataPower {
                    decompressed_size: 1,
                    compressed_size: 1,
                },
                description: vec![CompressDataDescription {
                    start_byte: 0,
                    amount_bytes: 1,
                    method: "00".to_string(),
                }],
            });
        } else {
            best_compress_for_first_n_bytes.push(CompressData {
                power: CompressDataPower {
                    decompressed_size: 1,
                    compressed_size: 2,
                },
                description: vec![CompressDataDescription {
                    start_byte: 0,
                    amount_bytes: 1,
                    method: "01".to_string(),
                }],
            });
        }

        for i in 1..self.bytes_info.len() {
            let mut current_best_compress = CompressData {
                power: CompressDataPower {
                    decompressed_size: best_compress_for_first_n_bytes[i - 1]
                        .power
                        .decompressed_size
                        + 1,
                    compressed_size: best_compress_for_first_n_bytes[i - 1].power.compressed_size
                        + 2,
                },
                description: vec![CompressDataDescription {
                    start_byte: i,
                    amount_bytes: 1,
                    method: "01".to_string(),
                }],
            };

            for j in (i..=std::cmp::max(0, i as isize - 63) as usize).rev() {
                let part_compress = self.compress_part(j, i);

                let mut prefix_compress = CompressData {
                    power: CompressDataPower::default(),
                    description: Vec::new(),
                };

                if part_compress.description[0].start_byte != 0 {
                    let prev_desc_index = part_compress.description[0].start_byte - 1;
                    prefix_compress.power = best_compress_for_first_n_bytes[prev_desc_index]
                        .power
                        .clone();
                    prefix_compress.description = best_compress_for_first_n_bytes[prev_desc_index]
                        .description
                        .clone();
                }

                if prefix_compress.power.range() + part_compress.power.range()
                    > current_best_compress.power.range()
                {
                    current_best_compress = CompressData {
                        power: CompressDataPower {
                            decompressed_size: prefix_compress.power.decompressed_size
                                + part_compress.power.decompressed_size,
                            compressed_size: prefix_compress.power.compressed_size
                                + part_compress.power.compressed_size,
                        },
                        description: [prefix_compress.description, part_compress.description]
                            .concat(),
                    };
                }
            }

            best_compress_for_first_n_bytes.push(current_best_compress);
        }

        CompressResult {
            uncompressed_data: format!("0x{}", self.data),
            compressed_data: self.zip(
                best_compress_for_first_n_bytes
                    .last()
                    .unwrap()
                    .description
                    .clone(),
            ),
            power: best_compress_for_first_n_bytes
                .last()
                .unwrap()
                .power
                .clone(),
            description: best_compress_for_first_n_bytes
                .last()
                .unwrap()
                .description
                .clone(),
        }
    }

    fn get_byte(&self, n: usize) -> Vec<u8> {
        self.get_bytes(n, 1)
    }

    fn get_bytes(&self, start: usize, n: usize) -> Vec<u8> {
        self.data[2 * start..2 * (start + n)].as_bytes().to_vec()
    }

    async fn init_dict(&mut self, size: usize, wallet: &str) {
        // self.dict = [
        //     ethers::utils::hexlify(wallet).unwrap(),
        //     ethers::utils::hexlify(self.contract.get_address()).unwrap(),
        //     ethers::utils::hexlify(self.contract.get_data(2, 2 + size)).unwrap(),
        // ];
        // self.lookup = {};
        // for (let i = 0; i < self.dict.len(); i++) {
        //     let value = self.dict[i].slice(2);
        //     self.lookup[value] = i;
        //     self.lookup[value.slice(-8)] = i;
        //     self.lookup[value.slice(-40)] = i;
        //     self.lookup[value.slice(-62)] = i;
        // }
    }

    pub fn check_zeros_case(&self, n: usize) -> CompressDataPower {
        let mut current_byte_index = n;
        let byte = self.get_byte(current_byte_index);
        if byte != "00".as_bytes() {
            return CompressDataPower {
                decompressed_size: 0,
                compressed_size: 0,
            };
        }
        current_byte_index += 1;
        while self.get_byte(current_byte_index) == "00".as_bytes()
            && current_byte_index < self.data.len() / 2
            && current_byte_index - n <= 63
        {
            current_byte_index += 1;
        }
        CompressDataPower {
            decompressed_size: current_byte_index - n,
            compressed_size: 1,
        }
    }

    pub fn check_copy_case_with_zeros(&self, n: usize) -> CompressDataPower {
        let mut current_byte_index = n;
        let byte = self.get_byte(current_byte_index);
        if byte != "00".as_bytes() {
            return CompressDataPower {
                decompressed_size: 1,
                compressed_size: 2,
            };
        }
        current_byte_index += 1;
        while self.get_byte(current_byte_index) == "00".as_bytes()
            && current_byte_index < self.data.len()
        {
            if current_byte_index - n == 32 {
                return CompressDataPower {
                    decompressed_size: 31,
                    compressed_size: 32,
                };
            }
            current_byte_index += 1;
        }
        let decompressed_bytes_amount = std::cmp::min(self.data.len() / 2 - n, 32);
        CompressDataPower {
            decompressed_size: decompressed_bytes_amount,
            compressed_size: if decompressed_bytes_amount == 32 {
                1 + 32 - (current_byte_index - n + 1)
            } else {
                1 + decompressed_bytes_amount
            },
        }
    }

    pub fn check_storage_case(&self, n: usize) -> anyhow::Result<Vec<CompressDataPower>> {
        if self.dict.is_empty() || self.lookup.is_empty() {
            return Err(anyhow::Error::msg("Dictionary is not initialized"));
        }

        let mut best = Vec::<CompressDataPower>::new();
        for len in [32, 31, 20, 4].iter() {
            let tail = self.get_bytes(n, *len);
            let index = self.lookup.get(&tail);
            if let Some(index) = index {
                if tail.len() / 2 >= *len {
                    best.push(CompressDataPower {
                        decompressed_size: *len,
                        compressed_size: if *index > 4096 { 3 } else { 2 },
                    });
                }
            }
        }
        Ok(best)
    }
}

struct CompressResult {
    uncompressed_data: String,
    compressed_data: String,
    power: CompressDataPower,
    description: Vec<CompressDataDescription>,
}

async fn compress(
    calldata: &str,
    decompressor_ext: &str,
    wallet: &str,
    init_dict_size: usize,
) -> CompressResult {
    let mut calldata_obj = Calldata::new(calldata, decompressor_ext).unwrap();
    calldata_obj.init_dict(init_dict_size, wallet);
    calldata_obj.compress()
}
