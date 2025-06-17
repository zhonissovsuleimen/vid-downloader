# Video Downloader

This project allows downloading videos from different media platforms.



## Currently supported platforms:
**Twitter / X** (requires [ffmpeg](https://www.ffmpeg.org/)) 
 - supports video downloads
 - supports multiple resolutions

**TikTok**
 - supports video downloads

## Setup
  1. Install [cargo](https://www.rust-lang.org/)
  2. Clone the repository

     ```bash
     git clone https://github.com/zhonissovsuleimen/vid-downloader
     ```
     
  3. Open the repository folder and build the project
     ```bash
     cargo build --release
     ```

     The executable can be found in target\release folder.


## Usage
- ```-i <link>``` to download a single video

```bash
vid-downloader.exe -i <link>
```

- ```-a``` allows input of multiple links
```bash
vid-downloader.exe -a
```

Additional arguments:

```-h``` to prefer high resolution

```-m``` to prefer medium resolution

```-l``` to prefer low resolution
