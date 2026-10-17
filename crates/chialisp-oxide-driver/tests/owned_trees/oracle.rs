//! Native-only bounded oracle. Parse into a flat arena and check BOTH children
//! before constructing any OwnedTree Box. Host limits return Err outside catch.
use chialisp_oxide::OwnedTree;
use std::fmt::Write as _;

enum Node {
    Atom(usize, usize),
    Pair(usize, usize),
}
struct Arena {
    wire: Vec<u8>,
    nodes: Vec<Node>,
    root: usize,
}

fn parse(text: &str) -> Result<Arena, String> {
    if text.len() > 2_200_000 || !text.is_ascii() {
        return Err("host serialized input limit".into());
    }
    if text.len() % 2 != 0 {
        return Err("odd hex".into());
    }
    let wire = (0..text.len())
        .step_by(2)
        .map(|offset| {
            u8::from_str_radix(&text[offset..offset + 2], 16).map_err(|_| "invalid hex".into())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut arena = Arena {
        wire,
        nodes: vec![],
        root: 0,
    };
    let mut offset = 0;
    let mut pending = Vec::<Option<usize>>::new();
    let mut atom_bytes = 0usize;
    loop {
        let byte = *arena.wire.get(offset).ok_or("truncated tree")?;
        offset += 1;
        if byte == 0xff {
            if pending.len() >= 128 {
                return Err("host whole-tree depth limit".into());
            }
            pending.push(None);
            continue;
        }
        let (start, length) = if byte < 0x80 {
            (offset - 1, 1)
        } else {
            let leading = byte.leading_ones() as usize;
            let mut length = usize::from(byte & ((1 << (7 - leading)) - 1));
            for _ in 1..leading {
                length = length
                    .checked_mul(256)
                    .and_then(|value| value.checked_add(usize::from(*arena.wire.get(offset)?)))
                    .ok_or("invalid atom length")?;
                offset += 1;
            }
            let start = offset;
            offset = offset.checked_add(length).ok_or("atom length overflow")?;
            if offset > arena.wire.len() {
                return Err("truncated atom".into());
            }
            (start, length)
        };
        atom_bytes = atom_bytes.checked_add(length).ok_or("atom byte overflow")?;
        if atom_bytes > 1_048_576 {
            return Err("host atom byte limit".into());
        }
        arena.nodes.push(Node::Atom(start, length));
        let mut value = arena.nodes.len() - 1;
        loop {
            if arena.nodes.len() > 4096 {
                return Err("host whole-tree node limit".into());
            }
            match pending.last_mut() {
                Some(slot @ None) => {
                    *slot = Some(value);
                    break;
                }
                Some(Some(first)) => {
                    let first = *first;
                    pending.pop();
                    arena.nodes.push(Node::Pair(first, value));
                    value = arena.nodes.len() - 1;
                }
                None => {
                    if offset != arena.wire.len() {
                        return Err("trailing serialized tree".into());
                    }
                    arena.root = value;
                    return Ok(arena);
                }
            }
        }
    }
}

fn owned(arena: &Arena, index: usize) -> OwnedTree<'_> {
    match arena.nodes[index] {
        Node::Atom(start, length) => OwnedTree::Atom(&arena.wire[start..start + length]),
        Node::Pair(first, rest) => {
            OwnedTree::Pair(Box::new(owned(arena, first)), Box::new(owned(arena, rest)))
        }
    }
}

fn list_depth(arena: &Arena, mut index: usize) -> Result<(), String> {
    let mut depth = 0;
    while let Node::Pair(_, rest) = arena.nodes[index] {
        depth += 1;
        if depth > 64 {
            return Err("host prefix recursion limit".into());
        }
        index = rest;
    }
    Ok(())
}

fn teardown(value: OwnedTree<'_>) {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        if let OwnedTree::Pair(first, rest) = value {
            pending.push(*first);
            pending.push(*rest);
        }
    }
}

fn encode(value: &OwnedTree<'_>) -> Result<Vec<u8>, String> {
    let mut output = vec![];
    let mut pending = vec![(value, 0)];
    let mut nodes = 0;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if nodes > 8192 || depth > 256 {
            return Err("host output tree limit".into());
        }
        match value {
            OwnedTree::Pair(first, rest) => {
                output.push(0xff);
                pending.push((rest, depth + 1));
                pending.push((first, depth + 1));
            }
            OwnedTree::Atom(bytes) => match bytes {
                [byte] if *byte < 128 => output.push(*byte),
                bytes if bytes.len() < 64 => {
                    output.push(0x80 | bytes.len() as u8);
                    output.extend_from_slice(bytes);
                }
                _ => return Err("host oracle output atom exceeds fixture limit".into()),
            },
        }
    }
    Ok(output)
}

fn execute(name: &str, arena: &Arena) -> Result<Option<Vec<u8>>, String> {
    // All host guards occur before Box creation and outside the policy catch.
    let arguments = if name == "prefix" {
        let Node::Pair(first, rest) = arena.nodes[arena.root] else {
            return Ok(None);
        };
        let Node::Pair(second, ending) = arena.nodes[rest] else {
            return Ok(None);
        };
        if !matches!(arena.nodes[ending], Node::Atom(_, 0)) {
            return Ok(None);
        }
        list_depth(arena, first)?;
        Some((first, second))
    } else {
        None
    };
    // Decoder construction is outside the catch as well: only the actual
    // checked policy call can produce the intentional panic/rejection outcome.
    let inputs = arguments.map(|(first, second)| (owned(arena, first), owned(arena, second)));
    let input = if arguments.is_none() {
        Some(owned(arena, arena.root))
    } else {
        None
    };
    let policy = std::panic::catch_unwind(move || {
        if let Some((first, second)) = inputs {
            wrapper_fixture::prefix(first, second)
        } else {
            let value = input.unwrap();
            match name {
                "head" => wrapper_fixture::head(value),
                "tail" => wrapper_fixture::tail(value),
                "construct" => wrapper_fixture::construct(value),
                "boxed_identity" => wrapper_fixture::boxed_identity(value),
                _ => unreachable!(),
            }
        }
    });
    match policy {
        Err(_) => Ok(None),
        Ok(value) => {
            let encoded = encode(&value);
            teardown(value);
            encoded.map(Some)
        }
    }
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = (|| {
        if args.len() != 2
            || !matches!(
                args[0].as_str(),
                "prefix" | "head" | "tail" | "construct" | "boxed_identity"
            )
        {
            return Err("expected supported operation and serialized-tree hex".into());
        }
        let arena = parse(&args[1])?;
        execute(&args[0], &arena)
    })();
    match result {
        Ok(None) => println!("rejected"),
        Ok(Some(bytes)) => {
            let mut text = String::new();
            for byte in bytes {
                write!(text, "{byte:02x}").unwrap();
            }
            println!("accepted {text}");
        }
        Err(error) => {
            eprintln!("host-inconclusive: {error}");
            std::process::exit(2);
        }
    }
}
