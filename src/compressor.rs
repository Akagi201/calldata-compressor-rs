use std::collections::HashMap;

use ethers::core::types::Bytes;
use num_bigint::BigUint;

use crate::errors::CompressorError;

type Bytes32 = [u8; 32];

/// How to compress a specific portion of data
#[derive(Debug, Clone)]
pub struct CompressDataDescription {
    pub start_byte: usize,   // starting byte index of the data portion to compress
    pub amount_bytes: usize, // number of bytes to compress starting from start_byte
    pub method: u8,          // compression method(decompress mask) to use
}

impl CompressDataDescription {
    pub fn new(start_byte: usize, amount_bytes: usize, method: u8) -> Self {
        CompressDataDescription {
            start_byte,
            amount_bytes,
            method,
        }
    }
}

/// the power of the compressed data
#[derive(Debug, Clone, Default)]
pub struct CompressDataPower {
    pub decompressed_size: usize, // the size of the original(decompresed) data in bytes.
    pub compressed_size: usize,   // the size of the compressed data in bytes.
}

impl CompressDataPower {
    pub fn new(decompressed_size: usize, compressed_size: usize) -> Self {
        CompressDataPower {
            decompressed_size,
            compressed_size,
        }
    }

    // the difference between the original(decompresed) data size and the compressed data size.
    pub fn range(&self) -> usize {
        self.decompressed_size - self.compressed_size
    }

    // adds the decompressed_size and compressed_size of another CompressDataPower instance to the current instance
    pub fn add(&mut self, other: &CompressDataPower) {
        self.decompressed_size += other.decompressed_size;
        self.compressed_size += other.compressed_size;
    }
}

/// the compressed data itself, along with its description and power
#[derive(Debug, Clone)]
pub struct CompressData {
    pub power: CompressDataPower, /* An instance of CompressDataPower representing the power of the compressed data. */
    pub description: Vec<CompressDataDescription>, /* An instance of CompressDataDescription representing the description of how the data was compressed. */
}

impl CompressData {
    pub fn new(power: CompressDataPower, description: &[CompressDataDescription]) -> Self {
        CompressData {
            power,
            description: description.to_vec(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ByteInfo {
    pub index: usize,
    pub zero_compress: CompressDataPower,
    pub copy_compress: CompressDataPower,
    pub storage_compress: Vec<CompressDataPower>,
}

impl ByteInfo {
    pub fn new(
        index: usize,
        zero_compress: CompressDataPower,
        copy_compress: CompressDataPower,
        storage_compress: &[CompressDataPower],
    ) -> Self {
        ByteInfo {
            index,
            zero_compress,
            copy_compress,
            storage_compress: storage_compress.to_vec(),
        }
    }
}

/// provide a tool to compress a hex string representing a smart contract call.
/// The compression is done by
#[derive(Debug, Clone)]
pub struct Calldata {
    pub data: Bytes,
    pub wallet_addr: Bytes32,
    pub contract_addr: Bytes32,
    pub bytes_info: Vec<ByteInfo>,
    pub dict: Vec<Bytes32>,              // contract dict data
    pub lookup: HashMap<Vec<u8>, usize>, // value -> index
}

impl Calldata {
    pub fn new(
        data: &Bytes,
        wallet_addr: &Bytes32,
        contract_addr: &Bytes32,
    ) -> Result<Self, CompressorError> {
        Ok(Calldata {
            data: data.clone(),
            wallet_addr: wallet_addr.clone(),
            contract_addr: contract_addr.clone(),
            bytes_info: Vec::new(),
            dict: Vec::new(),
            lookup: HashMap::new(),
        })
    }

    pub fn analyse(&mut self) -> Vec<ByteInfo> {
        self.bytes_info = vec![];
        for i in (0..self.data.len()).step_by(1) {
            self.bytes_info.push(ByteInfo {
                index: i,
                zero_compress: self.check_zeros_case(i),
                copy_compress: self.check_copy_case_with_zeros(i),
                storage_compress: self.check_storage_case(i).unwrap(),
            });
        }
        self.bytes_info.clone()
    }

    pub fn create_desc(
        &self,
        array_desc: &Vec<CompressDataDescription>,
        amount_bytes: usize,
        method: u8,
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
            method,
        }
    }

    pub fn add_just_copy_compress(
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
                0x01,
            ));
        }
        result_compress
    }

    pub fn compress_part(&self, from_byte: usize, to_byte: usize) -> CompressData {
        let mut part_compress = CompressData {
            power: CompressDataPower {
                decompressed_size: 0,
                compressed_size: 0,
            },
            description: vec![],
        };
        let mut just_copy_amount: usize = 0;

        let mut i = from_byte;
        while i <= to_byte {
            if self.bytes_info[i].zero_compress.decompressed_size >= to_byte - i + 1 {
                part_compress = self.add_just_copy_compress(part_compress, just_copy_amount);
                part_compress.power.add(&CompressDataPower {
                    decompressed_size: to_byte - from_byte + 1,
                    compressed_size: 1,
                });
                part_compress.description.push(CompressDataDescription {
                    start_byte: i,
                    amount_bytes: to_byte - i + 1,
                    method: 0x00,
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
                    let is_storage_range_more_than_copy_compress =
                        self.bytes_info[i].storage_compress[j].range()
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
                                    0x10
                                } else {
                                    0x11
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
                                0x00,
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
                                0x10
                            } else {
                                0x11
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
                            0x01,
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
                        0x00,
                    ));
                    i += zero_bytes_amount;
                } else if is_padding_with_copy {
                    part_compress
                        .power
                        .add(&self.bytes_info[i].clone().copy_compress);
                    part_compress.description.push(self.create_desc(
                        &part_compress.description,
                        self.bytes_info[i].copy_compress.decompressed_size,
                        0x01,
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

    pub fn zip(
        &self,
        instractions: Vec<CompressDataDescription>,
    ) -> Result<Vec<u8>, CompressorError> {
        let mut result: Vec<u8> = Vec::new();
        let bb = [32, 20, 4, 31];
        for instraction in instractions.iter() {
            match instraction.method {
                0x00 => {
                    // 00XXXXXX
                    result.push((instraction.amount_bytes - 1) as u8);
                }
                0x01 => {
                    // 01PXXXXX
                    let copy_bytes =
                        self.get_bytes(instraction.start_byte, instraction.amount_bytes);
                    let mut non_zero_byte_index = 0;
                    for j in 0..instraction.amount_bytes {
                        if copy_bytes[j] != 0x00 {
                            non_zero_byte_index = j;
                            break;
                        }
                    }
                    result.push(
                        ((instraction.amount_bytes - non_zero_byte_index - 1)
                            + 64
                            + if non_zero_byte_index == 0 { 0 } else { 32 })
                            as u8,
                    );
                    let copy_bytes = self.get_bytes(
                        instraction.start_byte + non_zero_byte_index,
                        instraction.amount_bytes - non_zero_byte_index,
                    );
                    result.extend(copy_bytes);
                }
                0x10 => {
                    // 10BBXXXX XXXXXXXX
                    let index = *self
                        .lookup
                        .get(&self.get_bytes(instraction.start_byte, instraction.amount_bytes))
                        .unwrap();
                    result.extend(
                        BigUint::from(
                            index
                                + 2_u64.pow(15) as usize
                                + (bb
                                    .iter()
                                    .position(|&r| r == instraction.amount_bytes)
                                    .unwrap()
                                    * 2_u64.pow(12) as usize),
                        )
                        .to_bytes_be(),
                    );
                }
                0x11 => {
                    // 11BBXXXX XXXXXXXX XXXXXXXX
                    let index = *self
                        .lookup
                        .get(&self.get_bytes(instraction.start_byte, instraction.amount_bytes))
                        .unwrap();
                    result.extend(
                        BigUint::from(
                            index
                                + 3 * 2_u64.pow(22) as usize
                                + (bb
                                    .iter()
                                    .position(|&r| r == instraction.amount_bytes)
                                    .unwrap()
                                    * 2_u64.pow(20) as usize),
                        )
                        .to_bytes_be(),
                    );
                }
                _ => {
                    return Err(CompressorError::UnsuportedMethod(instraction.method));
                }
            }
        }
        Ok(result)
    }

    pub fn compress(&mut self) -> Result<CompressResult, CompressorError> {
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
                    method: 0x00,
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
                    method: 0x01,
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
                    method: 0x01,
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

        Ok(CompressResult {
            uncompressed_data: self.data.clone(),
            compressed_data: Bytes::from(
                self.zip(
                    best_compress_for_first_n_bytes
                        .last()
                        .unwrap()
                        .description
                        .clone(),
                )?,
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
        })
    }

    pub fn get_byte(&self, n: usize) -> u8 {
        self.get_bytes(n, 1)[0]
    }

    pub fn get_bytes(&self, start: usize, n: usize) -> Vec<u8> {
        self.data.as_ref()[start..start + n].to_vec().clone()
    }

    pub fn init_dict(&mut self, dict: &[Bytes32]) {
        let mut dict_data = vec![self.wallet_addr, self.contract_addr];
        dict_data.extend(dict);
        self.dict = dict_data.clone();

        for (i, data) in self.dict.iter().enumerate() {
            let value: Vec<u8> = data[1..].to_vec();
            self.lookup.insert(value.clone(), i);
            self.lookup
                .insert(value.clone()[value.len() - 4..].to_vec(), i);
            self.lookup
                .insert(value.clone()[value.len() - 20..].to_vec(), i);
            self.lookup
                .insert(value.clone()[value.len() - 31..].to_vec(), i);
        }
    }

    pub fn check_zeros_case(&self, n: usize) -> CompressDataPower {
        let mut current_byte_index = n;
        let byte = self.get_byte(current_byte_index);
        if byte != 0x00 {
            return CompressDataPower {
                decompressed_size: 0,
                compressed_size: 0,
            };
        }
        current_byte_index += 1;
        while self.get_byte(current_byte_index) == 0x00
            && current_byte_index < self.data.len()
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
        if byte != 0x00 {
            return CompressDataPower {
                decompressed_size: 1,
                compressed_size: 2,
            };
        }
        current_byte_index += 1;
        while self.get_byte(current_byte_index) == 0x00 && current_byte_index < self.data.len() {
            if current_byte_index - n == 32 {
                return CompressDataPower {
                    decompressed_size: 31,
                    compressed_size: 32,
                };
            }
            current_byte_index += 1;
        }
        let decompressed_bytes_amount = std::cmp::min(self.data.len() - n, 32);
        CompressDataPower {
            decompressed_size: decompressed_bytes_amount,
            compressed_size: if decompressed_bytes_amount == 32 {
                1 + 32 - (current_byte_index - n + 1)
            } else {
                1 + decompressed_bytes_amount
            },
        }
    }

    pub fn check_storage_case(&self, n: usize) -> Result<Vec<CompressDataPower>, CompressorError> {
        if self.dict.is_empty() || self.lookup.is_empty() {
            return Err(CompressorError::DictNotInit);
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

pub struct CompressResult {
    pub uncompressed_data: Bytes,
    pub compressed_data: Bytes,
    pub power: CompressDataPower,
    pub description: Vec<CompressDataDescription>,
}

pub async fn compress(
    calldata: &Bytes,
    wallet_addr: &Bytes32,
    contract_addr: &Bytes32,
    dict: &[Bytes32],
) -> Result<CompressResult, CompressorError> {
    let mut calldata = Calldata::new(calldata, wallet_addr, contract_addr).unwrap();
    calldata.init_dict(dict);
    calldata.compress()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_new_calldata() {
        let result = Calldata::new(
            &Bytes::from(vec![0x00, 0x01, 0x02, 0x03]),
            &Bytes32::default(),
            &Bytes32::default(),
        );
        assert!(result.is_ok());
    }
}
