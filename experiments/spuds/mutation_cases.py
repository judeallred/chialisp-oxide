"""Actual source recipes for conditions whose enforcement belongs to consensus."""
RECIPES={
 'amount':('assert_my_amount(amount)','assert_my_amount(Amount::from_mojos(1_001))'),
 'age':('assert_height_relative(TAKE_WINDOW)','assert_height_relative(BlockAge::from_blocks(0))'),
 'nextstate':('instance_hash((module, next_key, next_payout, next_round))','instance_hash((module, next_key, holder_payout, next_round))'),
 'announcement':('create_coin_announcement(message)','create_coin_announcement(take_message(instance_hash((module, next_key, next_payout, round + 1))))'),
 'signature_binding':('            take_message(successor),','            claim_message(),'),
}

def witnesses(oracle):
    f=oracle.fixtures();module=bytes.fromhex(f['fixture_module_hash']);key=bytes.fromhex(f['holder_key']);payout=bytes.fromhex(f['holder_payout']);nk=bytes.fromhex(f['next_key']);np=bytes.fromhex(f['next_payout'])
    rows=[]
    for name in RECIPES:
        take=name in ('nextstate','signature_binding')
        base=oracle.step(module,key,payout,1,take,nk,np)
        changed=[list(c) for c in base]
        if name=='amount':changed[1][1]=1001
        if name=='age':changed[3][1]=0
        if name=='nextstate':
            changed=oracle.step(module,key,payout,1,True,nk,payout)
        if name=='announcement':
            changed[5][1]=oracle.sha(oracle.TAKE_DOMAIN+oracle.instance_hash(module,nk,np,2)).hex()
        if name=='signature_binding':
            changed[0][2]=oracle.sha(oracle.CLAIM_DOMAIN).hex();changed[5][1]=oracle.sha(oracle.CLAIM_DOMAIN).hex()
        env=f"((0x{module.hex()} 0x{key.hex()} 0x{payout.hex()} 1) {'1' if take else '()'} 0x{nk.hex()} 0x{np.hex()})"
        rows.append(dict(name=name,solution=env,module=module.hex(),key=key.hex(),payout=payout.hex(),next_key=nk.hex(),next_payout=np.hex(),round=1,take=take,original_conditions=base,mutant_conditions=changed))
    return rows
