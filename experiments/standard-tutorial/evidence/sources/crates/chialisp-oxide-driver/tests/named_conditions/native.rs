use super::*;
use clvm_traits::{ToClvm, clvm_list};
use clvmr_sdk::Allocator;

#[path = "oracle.rs"]
mod oracle;

#[test]
fn native_condition_fields_match_the_independent_sdk_schema() {
    for values in oracle::HEIGHT_CASES {
        let (relative, absolute, before_relative, before_absolute) = height_window(
            BlockAge::from_blocks(values[0]),
            BlockHeight::from_block_number(values[1]),
            BlockAge::from_blocks(values[2]),
            BlockHeight::from_block_number(values[3]),
        );
        let a = &mut Allocator::new();
        let actual = clvm_list!(
            clvm_list!(relative.0, relative.1.blocks()),
            clvm_list!(absolute.0, absolute.1.block_number()),
            clvm_list!(before_relative.0, before_relative.1.blocks()),
            clvm_list!(before_absolute.0, before_absolute.1.block_number()),
        )
        .to_clvm(a)
        .unwrap();
        oracle::assert_exact_conditions(a, actual, oracle::height_conditions(values));
    }
    for case in oracle::announcement_cases() {
        let (created, announced, own_id) =
            announcement_conditions(CoinId(case.origin), Message32(case.message));
        let a = &mut Allocator::new();
        let message = a.new_atom(&created.1.0).unwrap();
        let announcement = a.new_atom(&announced.1.0).unwrap();
        let coin_id = a.new_atom(&own_id.1.0).unwrap();
        let actual = clvm_list!(
            clvm_list!(created.0, message),
            clvm_list!(announced.0, announcement),
            clvm_list!(own_id.0, coin_id),
        )
        .to_clvm(a)
        .unwrap();
        oracle::assert_exact_conditions(a, actual, oracle::announcement_conditions(case));
        assert_eq!(discarded_announcement(CoinId(case.origin), Message32(case.message)), 7);
        assert_eq!(without_announcement(CoinId(case.origin), Message32(case.message)), 7);
    }
    // This explicit relabeling is legal Rust. The type catches accidental domain
    // swaps; neither the wrapper nor emission authenticates an announcement.
    for arbitrary in [[0; 32], [0xff; 32]] {
        let (condition,) = supplied_announcement(CoinAnnouncementId(CoinId(arbitrary).0));
        let a = &mut Allocator::new();
        let id = a.new_atom(&condition.1.0).unwrap();
        let actual = clvm_list!(clvm_list!(condition.0, id)).to_clvm(a).unwrap();
        oracle::assert_exact_conditions(a, actual, oracle::supplied_announcement(arbitrary));
    }
}
