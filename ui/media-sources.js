// Source IDs come from Windows SMTC, not from song titles or artists.
const sources=[
  [
    "applemusic",
    "Apple Music",
    [
      "applemusic",
      "apple.music"
    ]
  ],
  [
    "itunes",
    "iTunes",
    [
      "itunes"
    ]
  ],
  [
    "spotify",
    "Spotify",
    [
      "spotify"
    ]
  ],
  [
    "brave",
    "Brave",
    [
      "brave"
    ]
  ],
  [
    "edge",
    "Microsoft Edge",
    [
      "msedge",
      "microsoftedge",
      "microsoft.edge"
    ]
  ],
  [
    "vivaldi",
    "Vivaldi",
    [
      "vivaldi"
    ]
  ],
  [
    "opera",
    "Opera",
    [
      "opera"
    ]
  ],
  [
    "firefox",
    "Firefox",
    [
      "firefox"
    ]
  ],
  [
    "chrome",
    "Google Chrome",
    [
      "chrome",
      "chromium"
    ]
  ],
  [
    "youtubemusic",
    "YouTube Music",
    [
      "youtubemusic",
      "youtube-music",
      "youtube.music"
    ]
  ],
  [
    "amazonmusic",
    "Amazon Music",
    [
      "amazonmusic",
      "amazon.music"
    ]
  ],
  [
    "tidal",
    "TIDAL",
    [
      "tidal"
    ]
  ],
  [
    "soundcloud",
    "SoundCloud",
    [
      "soundcloud"
    ]
  ],
  [
    "vlc",
    "VLC",
    [
      "vlc",
      "videolan"
    ]
  ]
];
export function mediaSourceInfo(source=''){
 const id=String(source||'').toLowerCase();
 const match=sources.find(([, ,aliases])=>aliases.some(alias=>id.includes(alias)));
 return match?{icon:match[0],label:match[1],known:true}:{icon:'media',label:source?'Media player · '+source:'Windows media player',known:false};
}
