use chialisp_oxide::{PuzzleHash,PublicKey};
fn probe()->Vec<u8>{let x=chialisp_oxide_spuds::step((PuzzleHash([170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170,170]),PublicKey([139,198,30,147,56,15,52,39,237,42,206,129,63,223,246,117,4,245,82,104,169,154,114,206,165,113,51,194,179,42,29,131,111,213,35,129,174,156,161,9,87,85,123,128,6,64,89,229]),PuzzleHash([17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17]),1),true,PublicKey([180,127,161,225,81,98,165,177,249,68,77,26,174,87,209,49,161,213,201,139,173,232,248,233,59,142,191,225,100,155,149,197,87,135,184,115,29,47,26,6,56,179,225,210,58,218,72,232]),PuzzleHash([34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34]));
        list(vec![list(vec![uint(x.0.0.into()),atom(&x.0.1.0),atom(&x.0.2.0)]),c2(x.1.0.into(),x.1.1.mojos()),c2(x.2.0.into(),x.2.1.blocks().into()),c2(x.3.0.into(),x.3.1.blocks().into()),create(x.4.0.into(),&x.4.1.0,x.4.2.mojos()),list(vec![uint(x.5.0.into()),atom(&x.5.1.0)])])}

fn atom(v:&[u8])->Vec<u8>{if v.len()==1&&v[0]<128{return v.to_vec()}if v.len()<64{let mut out=vec![128+v.len() as u8];out.extend(v);out}else{panic!("finite atom bound")}}
fn uint(v:u64)->Vec<u8>{let b=v.to_be_bytes();let i=b.iter().position(|x|*x!=0).unwrap_or(8);let mut x=b[i..].to_vec();if x.first().is_some_and(|x|x&128!=0){x.insert(0,0);}atom(&x)}
fn list(xs:Vec<Vec<u8>>)->Vec<u8>{let mut t=vec![128];for x in xs.into_iter().rev(){let mut p=vec![255];p.extend(x);p.extend(t);t=p;}t}
fn c2(op:u64,v:u64)->Vec<u8>{list(vec![uint(op),uint(v)])}
fn create(op:u64,p:&[u8],v:u64)->Vec<u8>{list(vec![uint(op),atom(p),uint(v)])}
fn main(){let result=std::panic::catch_unwind(probe);match result{Ok(v)=>{print!("{{\"status\":\"accepted\",\"output_hex\":\"");for b in v{print!("{b:02x}");}println!("\"}}");},Err(_)=>println!("{{\"status\":\"rejected\"}}")}}
