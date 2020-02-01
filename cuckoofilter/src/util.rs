use crate::bucket::{Fingerprint, FINGERPRINT_SIZE};
use byteorder::{BigEndian, WriteBytesExt};
use std::hash::{Hash, Hasher};

// A struct combining *F*ingerprint *a*nd *I*ndexes,
// to have a return type with named fields
// instead of a tuple with unnamed fields.
pub struct FaI {
    pub fp: Fingerprint,
    pub i1: usize,
    pub i2: usize,
}

fn get_hash<T: ?Sized + Hash, H: Hasher + Default>(data: &T) -> (u32, u32) {
    let mut hasher = <H as Default>::default();
    data.hash(&mut hasher);
    let result = hasher.finish();

    // split 64bit hash value in the upper and the lower 32bit parts,
    // one used for the fingerprint, the other used for the indexes.
    ((result >> 32) as u32, result as u32)
}

pub fn get_alt_index<H: Hasher + Default>(fp: Fingerprint, i: usize) -> usize {
    let (_, index_hash) = get_hash::<_, H>(&fp.data);
    let alt_i = index_hash as usize;
    (i ^ alt_i) as usize
}

impl FaI {
    fn from_data<T: ?Sized + Hash, H: Hasher + Default>(data: &T) -> FaI {
        let (fp_hash, index_hash) = get_hash::<_, H>(data);

        let mut fp_hash_arr = [0; 4];
        let _ = (&mut fp_hash_arr[..]).write_u32::<BigEndian>(fp_hash);
        let mut valid_fp_hash: [u8; FINGERPRINT_SIZE] = [0; FINGERPRINT_SIZE];
        let mut n = 0;
        let fp;

        // increment every byte of the hash until we find one that is a valid fingerprint
        loop {
            for i in 0..FINGERPRINT_SIZE {
                valid_fp_hash[i] = fp_hash_arr[i] + n;
            }

            if let Some(val) = Fingerprint::from_data(valid_fp_hash) {
                fp = val;
                break;
            }
            n += 1;
        }

        let i1 = index_hash as usize;
        let i2 = get_alt_index::<H>(fp, i1);
        FaI { fp, i1, i2 }
    }

    pub fn random_index<R: ::rand::Rng>(&self, r: &mut R) -> usize {
        if r.gen() {
            self.i1
        } else {
            self.i2
        }
    }
}

pub fn get_fai<T: ?Sized + Hash, H: Hasher + Default>(data: &T) -> FaI {
    FaI::from_data::<_, H>(data)
}

#[test]
fn test_fp_and_index() {
    use std::collections::hash_map::DefaultHasher;
    let data = "seif";
    let fai = get_fai::<_, DefaultHasher>(data);
    let fp = fai.fp;
    let i1 = fai.i1;
    let i2 = fai.i2;
    let i11 = get_alt_index::<DefaultHasher>(fp, i2);
    assert_eq!(i11, i1);

    let i22 = get_alt_index::<DefaultHasher>(fp, i11);
    assert_eq!(i22, i2);
}
