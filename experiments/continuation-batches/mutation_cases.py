"""Exact actual Rust-source mutation recipes and independent finite witnesses."""
RECIPES={
 'second_opening':('    assert!(item(second_recipient, next).0 == middle.0);','    let _ = item(second_recipient, next);'),
 'amount':('assert_my_amount(remaining_budget(remaining))','assert_my_amount(Amount::from_mojos(4_001))'),
 'terminal':('        assert!(next.0 == terminal().0);','        let _ = next;'),
 'successor':('instance_hash((module, next, count))','instance_hash((module, head, count))'),
}

def witnesses(oracle):
    f=oracle.fixtures();row=f['steps'][0];module=bytes.fromhex('aa'*32)
    rows=[]
    for name in RECIPES:
        count=2 if name=='terminal' else 4
        second='99'*32 if name=='second_opening' else row['second']
        env=f"((0x{module.hex()} 0x{row['head']} {count}) ((0x{row['first']} 0x{row['middle']}) (0x{second} 0x{row['next']})))"
        base=oracle.step(module,bytes.fromhex(row['head']),4,bytes.fromhex(row['first']),bytes.fromhex(row['middle']),bytes.fromhex(row['second']),bytes.fromhex(row['next']))
        changed=[list(c) for c in base]
        if name=='second_opening':changed[2][1]=second
        if name=='amount':changed[0][1]=4001
        if name=='terminal':changed=changed[:3];changed[0][1]=2000
        if name=='successor':changed[3][1]=oracle.instance_hash(module,bytes.fromhex(row['head']),2).hex()
        rows.append(dict(name=name,solution=env,module=module.hex(),head=row['head'],remaining=count,first=row['first'],middle=row['middle'],second=second,next=row['next'],original_conditions=None if name in ('terminal','second_opening') else base,mutant_conditions=changed))
    return rows
