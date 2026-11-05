//! Host construction, independent of the on-chain source's expanded hash math.
use anyhow::Result;
use clvmr::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};

fn list(allocator: &mut Allocator, items: &[NodePtr]) -> Result<NodePtr> {
    let mut result = NodePtr::NIL;
    for item in items.iter().rev() {
        result = allocator.new_pair(*item, result)?;
    }
    Ok(result)
}

pub fn assemble(source: &str) -> Result<Vec<u8>> {
    let mut allocator = Allocator::new();
    let node = rue_clvm::assemble(&mut allocator, source)?;
    Ok(node_to_bytes(&allocator, node)?)
}

pub fn atom(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut allocator = Allocator::new();
    let node = allocator.new_atom(bytes)?;
    Ok(node_to_bytes(&allocator, node)?)
}

/// Materialize the standard curry tree, including the real argument trees.
pub fn curry(module: &[u8], arguments: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut allocator = Allocator::new();
    let module = node_from_bytes(&mut allocator, module)?;
    let quote = allocator.new_atom(&[1])?;
    let apply = allocator.new_atom(&[2])?;
    let cons = allocator.new_atom(&[4])?;
    let mut environment = quote;
    for argument in arguments.iter().rev() {
        let argument = node_from_bytes(&mut allocator, argument)?;
        let quoted = allocator.new_pair(quote, argument)?;
        environment = list(&mut allocator, &[cons, quoted, environment])?;
    }
    let quoted = allocator.new_pair(quote, module)?;
    let program = list(&mut allocator, &[apply, quoted, environment])?;
    Ok(node_to_bytes(&allocator, program)?)
}

/// A StandardLayer delegated-puzzle solution, signed by the synthetic key.
pub fn standard_solution(conditions: &str) -> Result<Vec<u8>> {
    assemble(&format!("(() (1 . {conditions}) ())"))
}
