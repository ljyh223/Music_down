
use std::io::Cursor;
use image::ImageFormat;
use indexmap::IndexMap;
use std::net::{TcpStream, SocketAddr};

pub fn merged_lyric(lyric: String, tlyric: String) -> String {
    let lyric_t=lyric.to_owned().trim_end().to_string();
    let tlyric_t=tlyric.to_owned().trim_end().to_string();
    let mut tlyric_map: IndexMap<String, String> = IndexMap::new();
    //读取每行
    for line in tlyric_t.lines() {
        // println!("{}",line);
        let parts:Vec<&str> = line.splitn(2, "]").collect();
        if parts.len()==2 {
            let time = &parts.get(0).unwrap().to_string()[1..];
            let text = parts.get(1).unwrap().to_string();
            tlyric_map.insert(time.to_owned(), text);
        }
        

        
    }


    // println!("{}",lyric_t.lines().collect::<Vec<_>>().len());
    // println!("{}",tlyric_t.lines().collect::<Vec<_>>().len());
    let mut merged: String = String::new();
    // let mut lyric_map: IndexMap<String, String> = IndexMap::new();

    for line in lyric_t.lines() {
        let mut parts = line.splitn(2, "]");
        //时间轴
        let time = &parts.next().unwrap()[1..];
        //歌词文本
        let text = parts.next().unwrap().to_string();
        
        if !tlyric_map.contains_key(time) {
            merged.push_str(&format!("[{}]{}\n", &time, text.as_str()));
        } else {
            merged.push_str(&format!(
                "[{}]{}\n[{}]{}\n",
                time,
                text.as_str(),
                time,
                tlyric_map.get(time).unwrap()
            ));
        }
    }
    merged
    
}
pub fn sy_re(s: String) -> String {
    let v2: Vec<[&str; 2]> = vec![
        ["<", "＜"],
        [">", "＞"],
        ["\\", "＼"],
        ["/", "／"],
        [":", "："],
        ["?", ""],
        ["*", "＊"],
        ["\"", "＂"],
        ["|", "｜"],
        ["*", ""],
        ["...", " "],
        ["?",""],
    ];
    v2.iter().fold(s, |acc, &[from, to]| acc.replace(from, to))
}
pub fn check_png(data:&Vec<u8>) ->Result<Vec<u8>, Box<dyn std::error::Error>>{
    let png_image = image::load_from_memory(&data)?;

    // Create a buffer to store the JPEG data
    let mut jpg_data: Vec<u8> = Vec::new();

    // Encode the DynamicImage as JPEG and write it into the jpg_data buffer
    let _ = png_image.write_to(&mut Cursor::new(&mut jpg_data), ImageFormat::Jpeg);
    return Ok(jpg_data);
    

}
pub fn is_port_open(port: u16) -> Result<bool, std::io::Error> {

    let addr = SocketAddr::from(([127, 0, 0, 1], port)); // IPv4 loopback address

    match TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(1)) {
        Ok(_) => Ok(true), // If the connection succeeds, the port is open.
        Err(error) => {
            if error.kind() == std::io::ErrorKind::ConnectionRefused ||
               error.kind() == std::io::ErrorKind::TimedOut {
                Ok(false) // The connection was refused or timed out, so the port is likely closed.
            } else {
                Err(error) // Some other IO error occurred; return it.
            }

        },

    }

}

