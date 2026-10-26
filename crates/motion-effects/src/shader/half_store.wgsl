// A host-selected Float16 attachment receives an already representable value.
// Native floating-point attachment conversion need not use nearest-even.
fn sdk_half(v:f32)->f32 {
    let bits=bitcast<u32>(v);let sign=bits&0x80000000u;let magnitude=bits&0x7fffffffu;
    if magnitude>=0x7f800000u{return v;}
    if magnitude>=0x47800000u{return bitcast<f32>(sign|0x7f800000u);}
    if magnitude<0x38800000u {
        let scaled=abs(v)*16777216.0;let lower=floor(scaled);let fraction=scaled-lower;
        let increment=fraction>0.5||(fraction==0.5&&(u32(lower)&1u)==1u);
        let q=(lower+select(0.0,1.0,increment))/16777216.0;
        return bitcast<f32>(bitcast<u32>(q)|sign);
    }
    let rounded=(magnitude+0xfffu+((magnitude>>13u)&1u))&0x7fffe000u;
    if rounded>=0x47800000u{return bitcast<f32>(sign|0x7f800000u);}
    return bitcast<f32>(sign|rounded);
}
