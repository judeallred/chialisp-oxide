use chialisp_oxide_authenticated_continuations as p;
use chialisp_oxide::{PuzzleHash,Message32};
fn atom(b:&[u8])->Vec<u8>{if b.is_empty(){vec![128]}else if b.len()==1&&b[0]<=127{b.to_vec()}else{let mut v=vec![128+u8::try_from(b.len()).unwrap()];v.extend(b);v}}
fn uint(n:u64)->Vec<u8>{if n==0{return atom(&[])}let b=n.to_be_bytes();let first=b.iter().position(|v|*v!=0).unwrap();let mut payload=b[first..].to_vec();if payload[0]&128!=0{payload.insert(0,0)}atom(&payload)}
fn list(items:&[Vec<u8>])->Vec<u8>{let mut v=Vec::new();for item in items{v.push(255);v.extend(item)}v.push(128);v}
fn main(){
let result=std::panic::catch_unwind(||p::step((PuzzleHash([0xaa;32]),Message32([126,18,70,22,152,78,188,100,105,123,194,205,76,154,215,78,165,144,124,232,165,223,144,15,249,126,132,44,254,100,195,196]),4),(PuzzleHash([17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17]),Message32([169,156,208,213,48,56,135,180,178,51,163,204,83,7,253,242,9,195,207,26,145,29,188,72,134,212,67,98,179,52,220,193]))));
match result{Err(_)=>println!(r#"{{"status":"native_panic"}}"#),Ok(out)=>{let mut conditions=vec![list(&[uint(out.first.0),atom(&out.first.1.0),uint(out.first.2.mojos())])];
        if let Some(next)=out.rest {conditions.push(list(&[uint(next.0),atom(&next.1.0),uint(next.2.mojos())]));}
let hex=list(&conditions).iter().map(|b|format!("{b:02x}")).collect::<String>();
println!(r#"{{"status":"accepted","output_hex":"{hex}"}}"#);}}}
