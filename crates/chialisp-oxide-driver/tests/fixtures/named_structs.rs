#![allow(dead_code)]

pub struct Terms {
    pub small: u8,
    pub wide: u64,
    pub flag: bool,
}
#[repr(transparent)]
pub struct Amount(pub u64);
pub struct Nested {
    pub terms: Terms,
    pub units: (i16, (), [u8; 3]),
    pub amount: Amount,
}
pub struct Cons {
    pub rest: u16,
    pub first: u8,
}

#[chialisp_oxide::puzzle]
pub fn roundtrip(value: Terms) -> Terms {
    value
}
#[chialisp_oxide::puzzle]
pub fn construct(small: u8, wide: u64, flag: bool) -> Terms {
    // Source initializer order is distinct from declaration/ABI order.
    Terms { flag, wide, small }
}
#[chialisp_oxide::puzzle]
pub fn nested(value: Nested) -> Nested {
    value
}
#[chialisp_oxide::puzzle]
pub fn update(mut value: Nested, delta: u8) -> (Nested, u8) {
    let old = value.terms.small;
    value.terms.small = value.terms.small + delta;
    value.units.0 = value.units.0 - 1;
    value.amount.0 = value.amount.0 + 2;
    (value, old)
}
#[chialisp_oxide::puzzle]
pub fn moved(value: Terms, replacement: u8) -> (Terms, Terms) {
    let old = value;
    let value = Terms {
        small: replacement,
        wide: old.wide + 1,
        flag: !old.flag,
    };
    (old, value)
}
#[chialisp_oxide::puzzle]
pub fn ignored(value: Terms) -> u8 {
    let _ = value;
    7
}
#[chialisp_oxide::puzzle]
pub fn strict(divisor: u8) -> u8 {
    let _discarded = Terms {
        flag: true,
        wide: 1,
        small: 9 / divisor,
    };
    7
}
#[chialisp_oxide::puzzle]
pub fn same_name(value: Cons) -> Cons {
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_native_values_and_snapshots() {
        let result = roundtrip(Terms {
            small: 7,
            wide: 513,
            flag: true,
        });
        assert_eq!((result.small, result.wide, result.flag), (7, 513, true));
        let result = construct(7, 513, true);
        assert_eq!((result.small, result.wide, result.flag), (7, 513, true));
        let value = Nested {
            terms: Terms {
                small: 7,
                wide: 513,
                flag: true,
            },
            units: (-129, (), [0, 128, 255]),
            amount: Amount(99),
        };
        let value = nested(value);
        let (updated, old) = update(value, 2);
        assert_eq!(old, 7);
        assert_eq!(
            (updated.terms.small, updated.terms.wide, updated.terms.flag),
            (9, 513, true)
        );
        assert_eq!(updated.units, (-130, (), [0, 128, 255]));
        assert_eq!(updated.amount.0, 101);
        let (old, new) = moved(
            Terms {
                small: 7,
                wide: 513,
                flag: true,
            },
            2,
        );
        assert_eq!((old.small, old.wide, old.flag), (7, 513, true));
        assert_eq!((new.small, new.wide, new.flag), (2, 514, false));
        assert_eq!(ignored(old), 7);
        assert_eq!(strict(3), 7);
        assert!(std::panic::catch_unwind(|| strict(0)).is_err());
        let value = same_name(Cons {
            rest: 513,
            first: 7,
        });
        assert_eq!((value.rest, value.first), (513, 7));
    }
}
