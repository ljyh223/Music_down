mod utils;
use id3::frame::{Content, Lyrics, Picture, PictureType};
use id3::{Error, ErrorKind, Frame, TagLike, Version};
use metaflac::block::PictureType::Media;
use std::collections::HashMap;
use std::fs::{create_dir_all, File};

use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
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

impl MusicDownload {
    pub fn new(_id: String) -> MusicDownload {
        let option: Value = serde_json::from_str(&fs::read_to_string("./option.json").unwrap())
            .expect("init json loads error");

        MusicDownload {
            palylist_id: _id.clone(),
            playlist_name: String::new(),
            save_path: option["music_path"].as_str().unwrap().to_string(),
            music_info_path: option["music_path"].as_str().unwrap().to_string() + &_id + ".json",
            option,
            //需要下载
            require_music: HashMap::new(),
            all_music: HashMap::new(),
            //已经下载
            already_music: HashMap::new(),
        }
    }
    pub fn init(&mut self) {
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
    }
    pub async fn run(&mut self) {
        let _ = self.get_song_list().await;
        println!("请记得结束node进程");
    }
    async fn get_song_list(&mut self) -> Result<(), reqwest::Error> {
        let resp = reqwest::get(format!(
            "http://localhost:3000/playlist/detail?id={}",
            self.palylist_id
        ))
        .await?
        .json::<Value>()
        .await?;

        self.playlist_name = resp["playlist"]["name"].as_str().unwrap().to_string();

        //收集
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
                    donw_url: String::new(),
                    file_type: String::new(),
                };
                all_music.insert(_i["id"].as_i64().unwrap().to_string(), music);
            }
        }
        self.all_music = all_music;
        println!("all_music {:#?}", &self.all_music.len());

        //看看是否第一次下载，是否存在文件
        if !Path::new(&self.music_info_path).exists() {
            let _file = File::create(&self.music_info_path).expect("id file create error");
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
        }
        let _am: Vec<Music> = self.all_music.values().into_iter().cloned().collect();
        //将写入的json
        let body: Value = json!({
            "total" : self.all_music.len(),
            "data" : _am
        });

        let write_body = serde_json::to_string(&body).unwrap();
        //求补集

        self.require_music = diff_hashmap(&self.all_music, &self.already_music);
        println!("预计下载: {}", &self.require_music.len());
        if !self.require_music.is_empty() {
            let _ = self.down_song().await;
            let mut file1 = fs::File::create(&self.music_info_path).expect("111");
            let _ = file1.write_all(&write_body.as_bytes());
        } else {
            println!("没有需要下载的");
        }

        Ok(())
    }
    async fn down_song(&mut self) -> Result<(), reqwest::Error> {
        let client: reqwest::Client = reqwest::Client::new();
        //遍历出所有id
        let _ids: Vec<_> = self.require_music.keys().map(|x| x.to_owned()).collect();
        let resp = client
            .get(format!(
                "http://localhost:3000/song/url?id={}",
                _ids.join(",")
            ))
            .header(
                header::COOKIE,
                fs::read_to_string(self.option["cookie_path"].as_str().unwrap())
                    .unwrap()
                    .as_str(),
            )
            .send()
            .await?
            .json::<Value>()
            .await?;
        //获取data中的所有id和下载url
        let mut data = HashMap::new();
        for i in resp["data"].as_array().unwrap() {
            data.insert(
                i["id"].as_i64().unwrap().to_string(),
                i["url"].as_str().unwrap().to_string(),
            );
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
            let name=utils::sy_re(_music.name.clone());
            let filename = format!(
                "{}/{}.{}",
                self.playlist_name, name, _music.file_type
            );
            print!("{}",name);
            let filepath = self.save_path.clone() + &filename;
            //音乐数据
            let music_data = reqwest::get(_music.donw_url.to_string())
                .await?
                .bytes()
                .await?;
            //写入,创建子目录
            if !Path::new(&self.save_path).exists() {
                let path = Path::new(&filepath);
                let prefix = path.parent().unwrap();
                create_dir_all(prefix).unwrap();
            }
            let mut file = File::create(&filepath).unwrap();
            let _ = file.write_all(&music_data);
            //编辑标签
            let _ = self.edit_tag(&filepath, _music).await;

            println!("{:#?} ==> 下载完成", _music.name);
        }
        Ok(())
    }
    async fn edit_tag(
        &mut self,
        filename: &str,
        music: &Music,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

                if self.option["mp3"]["translation"].as_bool().unwrap()
                    && resp.has_key("pureMusic")
                    && resp["pureMusic"].as_bool().unwrap()
                    && resp["tlyric"].has_key("lyric")
                    && resp["tlyric"]["lyric"].as_str().unwrap() != ""
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
                // encoding=3, mime="image/jpeg", type=6, desc=u"Cover", data=pic_datav
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
        } else {
            //
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

                tag.add_picture("image/jpeg", Media, data);
            }

            let comment = tag.vorbis_comments_mut();

            if self.option["flac"]["lyric"].as_bool().unwrap() {
                let resp = reqwest::get(format!("http://localhost:3000/lyric/?id={}", music.id))
                    .await?
                    .json::<Value>()
                    .await?;
                let mut lyric = resp["lrc"]["lyric"].as_str().unwrap().to_string();
                if self.option["mp3"]["translation"].as_bool().unwrap()
                    && resp.has_key("pureMusic")
                    && resp["pureMusic"].as_bool().unwrap()
                    && resp["tlyric"].has_key("lyric")
                    && resp["tlyric"]["lyric"].as_str().unwrap() != "" 
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
        self.already_music.insert(music.id.clone(), music.clone());
        self.save_musin_info();
        Ok(())
    }

    fn save_musin_info(&self) {
        let mut file = File::create(&self.music_info_path).expect("save music info :open error");
        let data: Vec<Music> = self.already_music.values().into_iter().cloned().collect();
        let body = json!({"total":&self.already_music.len(),"data":data});
        let _ = file.write_all(serde_json::to_string(&body).unwrap().as_bytes());
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

#[tokio::main]
async fn main() {
    let mut env = std::env::args();
    if env.len() == 1 {
        print!("Give me your id");
        return;
    }
    let id = env.nth(1).unwrap();
    let mut down = MusicDownload::new(id);
    down.init();
    down.run().await;
}
