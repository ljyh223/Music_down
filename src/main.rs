mod utils;
use ansi_term::Colour;
use id3::frame::{Content, Lyrics, Picture, PictureType};
use id3::{Error, ErrorKind, Frame, TagLike, Version};
use metaflac::block::PictureType::{CoverFront, Media};
use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct Music {
    pub id: String,
    pub name: String,
    pub pic_url: String,
    pub singer: String,
    pub album: String,
    pub donw_url: String,
    pub file_type: String,
}
pub struct MusicDownload {
    pub playlist_pic_url: String,
    pub palylist_id: String,
    pub playlist_name: String,
    pub save_path: String,
    pub music_info_path: String,
    pub option: Value,
    //需下载
    pub require_music: HashMap<String, Music>,
    //总
    pub all_music: HashMap<String, Music>,
    //已
    pub already_music: HashMap<String, Music>,
}

trait HasKey {
    fn has_key(&self, key: &str) -> bool;
}

impl HasKey for Value {
    fn has_key(&self, key: &str) -> bool {
        match self {
            Value::Object(map) => map.contains_key(key),
            _ => false,
        }
    }
}
// 6904724287
impl MusicDownload {
    pub fn new(_id: String) -> MusicDownload {
        let option: Value = serde_json::from_str(&fs::read_to_string("./option.json").unwrap())
            .expect("init json loads error");

        MusicDownload {
            //歌单封面
            playlist_pic_url: String::new(),
            //id
            palylist_id: _id.clone(),
            //歌单名字
            playlist_name: String::new(),
            //保存路径
            save_path: option["music_path"].as_str().unwrap().to_string(),
            //保存歌单信息的文件路径
            music_info_path: option["music_path"].as_str().unwrap().to_string() + &_id + ".json",
            //下载参数
            option,
            //需要下载
            require_music: HashMap::new(),
            all_music: HashMap::new(),
            //已经下载
            already_music: HashMap::new(),
        }
    }
    //开启NeteaseCloudMusicApi
    pub fn init(&mut self) -> bool {
        return match utils::is_port_open(3000) {
            Ok(_is_open) => {
                println!(
                    "已开启NeteaseCloudMusicApi ==> {}",
                    Colour::Green.paint("last")
                );
                return true;
            }

            Err(_e) => {
                if !Path::new(&self.option["NeteaseCloudMusicApi_path"].as_str().unwrap()).exists()
                {
                    println!(
                        "{}",
                        Colour::Red.paint("NeteaseCloudMusicApi 不存在, 请检查")
                    );
                    return false;
                }
                let argument = format!(
                    "{}app.js",
                    self.option["NeteaseCloudMusicApi_path"].as_str().unwrap()
                );
                let _cmd: std::process::Child = Command::new("node")
                    .arg(argument)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("shell run error");
                println!(
                    "已开启NeteaseCloudMusicApi ==> {}",
                    Colour::Green.paint("now")
                );
                true
            }
        };
    }
    pub async fn run(&mut self) {
        let _ = self.get_song_list().await;
        println!("请记得结束node进程");
    }
    async fn get_song_list(&mut self) -> Result<(), reqwest::Error> {
        //获取 所有的id
        let resp = reqwest::get(format!(
            "http://localhost:3000/playlist/detail?id={}",
            self.palylist_id
        ))
        .await?
        .json::<Value>()
        .await?;
        //歌单 基本信息
        self.playlist_name = resp["playlist"]["name"].as_str().unwrap().to_string();
        self.playlist_pic_url = resp["playlist"]["coverImgUrl"]
            .as_str()
            .unwrap()
            .to_string();

        //收集 music 的基本信息
        let mut all_music: HashMap<String, Music> = HashMap::new();
        for i in 0..=resp["playlist"]["trackCount"].as_i64().unwrap() / 50 {
            let resp = reqwest::get(format!(
                "http://localhost:3000/playlist/track/all?id={}&limit=50&offset={}",
                self.palylist_id,
                i * 50
            ))
            .await?
            .json::<Value>()
            .await?;

            for _i in resp["songs"].as_array().unwrap() {
                let ar = _i["ar"].as_array().unwrap();
                let ars = if ar.len() >= 3 {
                    ar.get(..3)
                } else {
                    ar.get(..)
                }
                .unwrap()
                .iter()
                .map(|obj| obj["name"].as_str().unwrap())
                .collect::<Vec<_>>();

                let music = Music {
                    id: _i["id"].as_i64().unwrap().to_string(),
                    name: format!(
                        "{} - {}",
                        ars.join(","),
                        utils::sy_re(_i["name"].as_str().unwrap().to_string())
                    ),
                    //限制图片大小
                    pic_url: _i["al"]["picUrl"].as_str().unwrap().to_string() + "?param=1400y1400",
                    singer: utils::sy_re(ars.join(",")),
                    album: _i["al"]["name"].as_str().unwrap().to_string(),
                    //这两个下面补上
                    donw_url: String::new(),
                    file_type: String::new(),
                };
                all_music.insert(_i["id"].as_i64().unwrap().to_string(), music);
            }
        }

        self.all_music = all_music;
        println!(
            "all_music ==> {}",
            Colour::Green.paint(&self.all_music.len().to_string())
        );

        //是否存在保存歌单信息的json文件
        if !Path::new(&self.music_info_path).exists() {
            let _ = File::create(&self.music_info_path).expect("id file create error");
        }
        //打开
        let mut file = fs::File::open(&self.music_info_path).expect("msg");
        let mut data = String::new();
        let _ = file.read_to_string(&mut data);
        let _sub_hashmap: HashMap<String, Music> = HashMap::new();
        if !data.is_empty() {
            let josns: serde_json::Value = serde_json::from_str(&data).expect("json load error");
            //取data，并Vec
            let sub_json: Vec<Value> = josns["data"].as_array().unwrap().clone();
            //在转换为json字符串
            let _sub = serde_json::to_string(&sub_json).unwrap();
            //在解析成Vec<Music>
            let mut sub_id = serde_json::from_str::<Vec<Music>>(&_sub).unwrap();
            //变成hashmap<String,Music>,这玩意相当于已经下载的music
            self.already_music = sub_id.drain(..).map(|x| (x.id.to_owned(), x)).collect();
            //把所有的value 取出来再转成Vec
        } else {
            self.already_music = HashMap::new();
        }

        self.require_music = diff_hashmap(&self.all_music, &self.already_music);
        println!(
            "预计下载 ==> {}",
            Colour::Green.paint(&self.require_music.len().to_string())
        );
        if !self.require_music.is_empty() {
            let _ = self.down_song().await;
            let _ = self.meta_complete(Some(true)).await;
        } else {
            println!("{}", Colour::Yellow.paint("没有需要下载的"));
        }

        Ok(())
    }
    async fn down_song(&mut self) -> Result<(), reqwest::Error> {
        let client: reqwest::Client = reqwest::Client::new();
        //遍历出所有id

        let mut file = std::fs::File::open(self.option["cookie_path"].as_str().unwrap()).unwrap();
        let mut cookie = String::new();
        file.read_to_string(&mut cookie).unwrap();
        // println!("cookie ==> {:#?}", cookie);
        let _ids: Vec<_> = self.require_music.keys().map(|x| x.to_owned()).collect();
        let resp = client
            .get(format!(
                "http://localhost:3000/song/url/v1?id={}&level=lossless",
                _ids.join(",")
            ))
            .header(header::COOKIE, cookie.trim())
            .send()
            .await?
            .json::<Value>()
            .await?;
        //获取data中的所有id和下载url

        let mut data = HashMap::new();
        for i in resp["data"].as_array().unwrap() {
            if let Some(_value) = i.get("url").and_then(|v| v.as_str()) {
                if i["url"].as_str().unwrap().to_string().pop().unwrap() != '.' {
                    data.insert(
                        i["id"].as_i64().unwrap().to_string(),
                        i["url"].as_str().unwrap().to_string(),
                    );
                } else {
                    println!(
                        "{} ==> {} : {} url -> {}",
                        Colour::Yellow.paint("下载链接异常"),
                        self.all_music[&i["id"].as_i64().unwrap().to_string()].name,
                        i["id"].as_i64().unwrap().to_string(),
                        i["url"].as_str().unwrap().to_string(),
                    );
                    data.insert(i["id"].as_i64().unwrap().to_string(), String::from("null"));
                    continue;
                }
            } else {
                println!(
                    "{} ==> {} -> {}",
                    Colour::Yellow.paint("下载连接不存在"),
                    self.all_music[&i["id"].as_i64().unwrap().to_string()].name,
                    i["id"].as_i64().unwrap().to_string()
                );
                data.insert(i["id"].as_i64().unwrap().to_string(), String::from("null"));
            }
        }
        let mut music = self.require_music.clone();

        for (key, value) in &data {
            if let Some(music_entry) = music.get_mut(key) {
                music_entry.donw_url = value.clone();
                music_entry.file_type = value
                    .clone()
                    .split(".")
                    .last()
                    .unwrap()
                    .to_ascii_lowercase();
            }
        }

        for (_id, _music) in music.iter() {
            let name = utils::sy_re(_music.name.clone());

            // 拼接一下歌单名
            let filepath_ex = self.save_path.clone() + &self.playlist_name;
            //判断歌单文件是否存在
            if !Path::new(&filepath_ex).exists() {
                // println!("path not exists");
                create_dir_all(&filepath_ex).unwrap();
            }
            //完整路径
            let filepath = format!("{}/{}.{}", filepath_ex, name, _music.file_type);
            //音乐数据
            if _music.donw_url == "null" {
                println!(
                    "{}/{} 检测到空url,已跳过 ==> {} ",
                    self.already_music.len(),
                    self.require_music.len(),
                    Colour::Yellow.paint(_music.name.to_string())
                );
                continue;
            }
            let music_data = reqwest::get(_music.donw_url.to_string())
                .await?
                .bytes()
                .await?;
            // 音乐数据写入
            let mut file = File::create(&filepath).unwrap();
            let _ = file.write_all(&music_data);

            self.already_music.insert(_music.id.clone(), _music.clone());
            self.save_musin_info();

            //编辑标签
            // let _ = self.edit_tag(&filepath, _music, Some(true)).await;
            let info = format!(
                "{}/{} 下载完成",
                self.already_music.len(),
                self.require_music.len()
            );
            println!("{} ==> {}", Colour::Green.paint(info), _music.name);
        }
        Ok(())
    }
    async fn edit_tag(
        &mut self,
        filename: &str,
        music: &Music,
        auto_save: Option<bool>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // println!("mate data :filename ==>{:#?}", &filename);
        if !Path::new(filename).exists() {
            println!("{} ==> {}", Colour::Yellow.paint("文件不存在"), filename);
            return Ok(());
        }
        if music.file_type.eq("mp3") {
            if !self.option["mp3"]
                .as_object()
                .unwrap()
                .values()
                .all(|v| v.as_bool().unwrap() == true)
            {
                return Ok(());
            }
            if self.option["mp3"]["translation"].as_bool().unwrap()
                && self.option["mp3"]["lyric"].as_bool().unwrap() == false
            {
                return Ok(());
            }
            let mut tag = match id3::Tag::read_from_path(&filename) {
                Ok(tag) => tag,
                Err(Error {
                    kind: ErrorKind::NoTag,
                    ..
                }) => id3::Tag::new(),
                Err(err) => return Err(Box::new(err)),
            };
            //歌词
            if self.option["mp3"]["lyric"].as_bool().unwrap() {
                let resp = reqwest::get(format!("http://localhost:3000/lyric/?id={}", music.id))
                    .await?
                    .json::<Value>()
                    .await?;
                let mut lyric = resp["lrc"]["lyric"].as_str().unwrap().to_string();
                // 用户true
                if self.option["mp3"]["translation"].as_bool().unwrap()
                    && resp["tlyric"].has_key("lyric")
                    && resp["tlyric"]["lyric"].as_str().unwrap() != ""
                    && (!resp.has_key("pureMusic") || !resp["pureMusic"].as_bool().unwrap())
                {
                    lyric = utils::merged_lyric(
                        lyric,
                        resp["tlyric"]["lyric"].as_str().unwrap().to_string(),
                    );
                }

                let l: Lyrics = Lyrics {
                    lang: String::from("chi"),
                    description: String::new(),
                    text: lyric,
                };

                tag.add_frame(Frame::with_content("USLT", Content::Lyrics(l.clone())));
            }

            //image
            if self.option["mp3"]["pic"].as_bool().unwrap() {
                let mut data = reqwest::get(music.pic_url.clone())
                    .await?
                    .bytes()
                    .await?
                    .to_vec();

                //png 第一位是137
                if data.get(0).unwrap().to_owned() == 137 {
                    //转换为jpg
                    match utils::check_png(&data) {
                        Ok(v) => data = v,
                        Err(_) => todo!(),
                    }
                }
                // encoding=3, mime="image/jpeg", type=6, desc=u"Cover", data=pic_datavet file=fs::File(&oath);
                let picture = Picture {
                    mime_type: String::from("image/jpeg"),
                    picture_type: PictureType::Media,
                    description: String::from("Cover"),
                    data,
                };

                tag.add_frame(Frame::with_content(
                    "APIC",
                    Content::Picture(picture.clone()),
                ));
            }

            //title
            if self.option["mp3"]["title"].as_bool().unwrap() {
                tag.set_album(music.album.to_string());
            }

            //artist
            if self.option["mp3"]["title"].as_bool().unwrap() {
                tag.set_artist(music.singer.to_string());
            }

            //save
            tag.write_to_path(&filename, Version::Id3v23)?;
        } else if music.file_type.eq("flac") {
            if !self.option["flac"]
                .as_object()
                .unwrap()
                .values()
                .all(|v| v.as_bool().unwrap())
            {
                return Ok(());
            }

            let mut tag = metaflac::Tag::read_from_path(&filename).unwrap();

            if self.option["flac"]["pic"].as_bool().unwrap() {
                let mut data = reqwest::get(music.pic_url.to_string())
                    .await?
                    .bytes()
                    .await?
                    .to_vec();
                //部分图片返回的是png的数据，导致win音乐不可见预览图片，同时也无法读取到元数据，aimp正常
                //png 第一位是137
                if data.get(0).unwrap().to_owned() == 137 {
                    //转换为jpg
                    match utils::check_png(&data) {
                        Ok(v) => data = v,
                        Err(_) => todo!(),
                    }
                }

                if cfg!(target_os = "windows") {
                    tag.add_picture("image/jpeg", Media, data);
                } else if cfg!(target_os = "linux") {
                    tag.add_picture("image/jpeg", CoverFront, data.clone());
                    tag.add_picture("image/jpeg", Media, data);
                }
            }

            let comment = tag.vorbis_comments_mut();
            if self.option["flac"]["lyric"].as_bool().unwrap() {
                let resp = reqwest::get(format!("http://localhost:3000/lyric/?id={}", music.id))
                    .await?
                    .json::<Value>()
                    .await?;
                let mut lyric = resp["lrc"]["lyric"].as_str().unwrap().to_string();

                if self.option["flac"]["translation"].as_bool().unwrap()
                    && resp["tlyric"].has_key("lyric")
                    && resp["tlyric"]["lyric"].as_str().unwrap() != ""
                    && (!resp.has_key("pureMusic") || !resp["pureMusic"].as_bool().unwrap())
                {
                    //可选，合并译文
                    let _tlyric = resp["tlyric"]["lyric"].as_str().unwrap();
                    lyric = utils::merged_lyric(lyric, _tlyric.to_string());
                }

                comment.set_lyrics(vec![lyric]);
            }

            if self.option["flac"]["title"].as_bool().unwrap() {
                comment.set_album(vec![music.album.to_string()]);
            }

            if self.option["flac"]["artist"].as_bool().unwrap() {
                comment.set_artist(vec![music.singer.to_string()]);
            }

            tag.save().unwrap();
        }
        match auto_save {
            Some(b) => {
                if b {
                    self.already_music.insert(music.id.clone(), music.clone());
                    self.save_musin_info();
                }
            }
            None => {}
        }

        Ok(())
    }

    fn save_musin_info(&self) {
        let mut file = File::create(&self.music_info_path).expect("save music info :open error");
        let data: Vec<Music> = self.already_music.values().into_iter().cloned().collect();
        let body = json!({
            "id":self.palylist_id,
            "name":self.playlist_name,
            "picUrl":self.playlist_pic_url,
            "total" : self.already_music.len(),
            "data" : data
        });
        let _ = file.write_all(serde_json::to_string(&body).unwrap().as_bytes());
    }
    pub async fn meta_complete(
        &mut self,
        already: Option<bool>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match already {
            Some(b) => {
                if b {
                    for (_, value) in self.already_music.clone() {
                        let filename1 = format!(
                            "{}{}/{}.{}",
                            self.save_path,
                            self.playlist_name,
                            utils::sy_re(value.name.clone()),
                            value.file_type
                        );
                        let _ = self.edit_tag(&filename1, &value, None).await;
                    }
                }
            }
            None => {
                let path = self.option["music_path"].as_str().unwrap().to_string()
                    + &self.palylist_id
                    + ".json";
                if Path::new(&path).exists() {
                    let f = File::open(&path).unwrap();
                    let json: Value = serde_json::from_reader(f).unwrap();

                    //---------------
                    if json.has_key("data") {
                        for i in json["data"].as_array().unwrap() {
                            let filename1 = format!(
                                "{}{}/{}.{}",
                                self.save_path,
                                json["name"].as_str().unwrap(),
                                utils::sy_re(i["name"].as_str().unwrap().to_string()),
                                i["file_type"].as_str().unwrap()
                            );
                            let music: Music = serde_json::from_value(i.clone()).unwrap();
                            let _ = self.edit_tag(&filename1, &music, None).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

pub fn diff_hashmap(
    map1: &HashMap<String, Music>,
    map2: &HashMap<String, Music>,
) -> HashMap<String, Music> {
    let mut m = HashMap::new();
    for (k, v) in map1 {
        if !map2.contains_key(k) {
            m.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in map2 {
        if !map1.contains_key(k) {
            m.insert(k.clone(), v.clone());
        }
    }
    m
}
// 6904724287
// 8677413940
#[tokio::main]
async fn main() {
    let env: Vec<String> = std::env::args().collect();
    if env.len() == 1 {
        print!("Give me your id");
        return;
    }
    let id = &env[2];
    let query = &env[1];
    let mut down = MusicDownload::new(id.clone());
    if !down.init() {
        return;
    }

    match query.as_str() {
        "down" => {
            down.run().await;
        }
        "tag" => {
            let _ = down.meta_complete(None).await;
        }
        _ => println!("{}", Colour::Red.paint("无效函数")),
    }
}
