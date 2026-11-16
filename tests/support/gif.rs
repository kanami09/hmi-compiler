// 标准 GIF89a 测试素材；简单 LZW 清表编码，不依赖原生 HMI 或私有资源。
#[derive(Clone)]
pub struct GifFrame {
    pub left:u16,pub top:u16,pub width:u16,pub height:u16,
    pub delay_cs:u16,pub disposal:u8,pub transparent:bool,pub pixels:Vec<u8>,
}
#[allow(dead_code)]
pub fn full(width:u16,height:u16,color:u8,delay_cs:u16)->GifFrame {
    GifFrame{left:0,top:0,width,height,delay_cs,disposal:1,transparent:false,pixels:vec![color;usize::from(width)*usize::from(height)]}
}
pub fn gif(width:u16,height:u16,frames:&[GifFrame])->Vec<u8> {
    let mut out=b"GIF89a".to_vec();out.extend(width.to_le_bytes());out.extend(height.to_le_bytes());
    out.extend([0x81,0,0]);out.extend([0,0,0,220,40,20,30,210,60,30,70,230]);
    for f in frames {
        assert_eq!(f.pixels.len(),usize::from(f.width)*usize::from(f.height));
        out.extend([0x21,0xf9,4,(f.disposal<<2)|u8::from(f.transparent)]);out.extend(f.delay_cs.to_le_bytes());out.extend([0,0,0x2c]);
        out.extend(f.left.to_le_bytes());out.extend(f.top.to_le_bytes());out.extend(f.width.to_le_bytes());out.extend(f.height.to_le_bytes());out.extend([0,2]);
        let mut data=Vec::new();let mut bits=0u32;let mut count=0;
        for code in f.pixels.iter().flat_map(|p|[4,*p]).chain([5]) {
            assert!(code<8);bits|=u32::from(code)<<count;count+=3;
            while count>=8 {data.push(bits as u8);bits>>=8;count-=8;}
        }
        if count>0 {data.push(bits as u8);}
        for chunk in data.chunks(255) {out.push(chunk.len()as u8);out.extend(chunk);}out.push(0);
    }
    out.push(0x3b);out
}
