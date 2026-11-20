// English / Indonesian for the Settings window. The page is written in English; when Indonesian is on, every text
// node and label under <body> is swapped through ID below, including text the code sets later (a MutationObserver
// watches). Text not in the table (names, paths, errors from Rust) stays as it is.
const KEY = 'sora.lang';
const ID = {
  // window, sidebar, headers
  'Search': 'Cari', 'Search settings': 'Cari pengaturan', 'Settings sections': 'Bagian pengaturan', 'Minimize window': 'Kecilkan jendela', 'Close window': 'Tutup jendela',
  'General': 'Umum', 'Settings': 'Pengaturan', 'Advanced': 'Lanjutan', 'Chat': 'Chat', "What's New": 'Yang Baru', 'Search results': 'Hasil pencarian', 'No settings match.': 'Tidak ada pengaturan yang cocok.',
  'Language, startup and updates.': 'Bahasa, startup, dan pembaruan.', 'Shortcut, placement and behavior.': 'Pintasan, posisi, dan perilaku.',
  'Coding agents in the notch. You approve every action.': 'Agent coding di notch. Setiap aksi butuh persetujuanmu.',
  'Record calls and transcribe them on this PC.': 'Rekam panggilan dan buat transkripnya di PC ini.', 'Reply to messages right from the notch.': 'Balas pesan langsung dari notch.',
  'What changed in this version.': 'Perubahan di versi ini.',
  // macOS
  'Shortcut and behavior.': 'Pintasan dan perilaku.', 'Open at login': 'Buka saat login', 'Hide over full screen': 'Sembunyikan saat layar penuh',
  'When an app or video fills the screen': 'Saat aplikasi atau video memenuhi layar', 'Shortcut, size and behavior.': 'Pintasan, ukuran, dan perilaku.',
  'Size': 'Ukuran', 'Notifications': 'Notifikasi', 'Small': 'Kecil', 'Large': 'Besar',
  'macOS permissions': 'Izin macOS', 'Full Disk Access': 'Akses Disk Penuh', 'Lets Sora read chat notifications from other apps.': 'Agar Sora bisa membaca notifikasi chat dari aplikasi lain.',
  'Screen & System Audio Recording': 'Rekaman Layar & Audio Sistem', 'Lets Sora record the sound of the call app.': 'Agar Sora bisa merekam suara aplikasi panggilan.',
  'Microphone': 'Mikrofon', 'Lets Sora record your voice.': 'Agar Sora bisa merekam suaramu.', 'Open': 'Buka',
  'Record calls and transcribe them on this Mac.': 'Rekam panggilan dan buat transkripnya di Mac ini.',
  // General
  'Language': 'Bahasa', 'Start with Windows': 'Jalan saat Windows mulai', 'Updates': 'Pembaruan', 'Sora updates': 'Pembaruan Sora',
  'Uninstall Sora': 'Uninstall Sora', 'Deletes Sora and everything it saved': 'Menghapus Sora dan semua yang disimpannya', 'Uninstall': 'Uninstall', 'Uninstall Sora?': 'Uninstall Sora?', 'Sora closes, and the app and everything it saved are deleted: settings, history, Board, reminders and connected agents. This cannot be undone.': 'Sora ditutup, lalu aplikasi dan semua yang disimpannya dihapus: pengaturan, riwayat, Board, pengingat, dan agent yang terhubung. Tidak bisa dibatalkan.',
  'Checks GitHub. Every download is verified.': 'Cek ke GitHub. Setiap unduhan diverifikasi.', 'Check for updates': 'Cek pembaruan', 'Checking…': 'Mengecek…', 'Check again': 'Cek lagi',
  'Updates are not set up for this build.': 'Pembaruan belum diatur untuk build ini.', 'You are up to date.': 'Sora sudah versi terbaru.', 'Could not check for updates.': 'Gagal mengecek pembaruan.',
  'Sora will restart when the download finishes.': 'Sora akan restart setelah unduhan selesai.', 'Could not save. Please try again.': 'Gagal menyimpan. Coba lagi.',
  'Startup preference unavailable. Reopen Settings to retry.': 'Pengaturan startup tidak tersedia. Buka ulang Settings.',
  // Notch
  'Shortcut': 'Pintasan', 'Show or hide the notch': 'Tampilkan atau sembunyikan notch', 'Record shortcut': 'Rekam pintasan', 'Placement': 'Posisi', 'Behavior': 'Perilaku',
  'Position': 'Letak', 'Left': 'Kiri', 'Center': 'Tengah', 'Right': 'Kanan', 'Display': 'Layar', 'Primary': 'Utama', 'Size': 'Ukuran',
  'Hide message text': 'Sembunyikan isi pesan', 'Chat cards say only what was sent, like "Sent a photo"': 'Kartu chat hanya menyebut jenisnya, seperti "Sent a photo"',
  'Hide when maximized': 'Sembunyikan saat jendela penuh', 'Edge handle': 'Pegangan tepi', 'A thin capsule stays while hidden': 'Kapsul tipis tetap tampil saat tersembunyi',
  'Game Mode': 'Mode Game', 'Hide Sora during games': 'Sembunyikan Sora saat main game', 'Off': 'Mati', 'Auto': 'Otomatis', 'Always': 'Selalu',
  'Press Ctrl first…': 'Tekan Ctrl dulu…', 'Press a shortcut…': 'Tekan pintasan…',
  'Hold Ctrl, optionally Alt or Shift, then a key. Esc cancels.': 'Tahan Ctrl (boleh plus Alt atau Shift), lalu satu tombol. Esc untuk batal.',
  'Hold Ctrl, Alt, Shift or Win, then a key. Esc cancels.': 'Tahan Ctrl, Alt, Shift, atau Win, lalu satu tombol. Esc untuk batal.',
  'Could not record a shortcut. Try again.': 'Gagal merekam pintasan. Coba lagi.', 'Press and hold Ctrl first.': 'Tekan dan tahan Ctrl dulu.',
  'Use Ctrl + key, with either Alt or Shift.': 'Pakai Ctrl + tombol, dengan Alt atau Shift.', 'Hold Ctrl, Alt, Shift or Win first.': 'Tahan Ctrl, Alt, Shift, atau Win dulu.',
  'Use a letter, number, F-key, or navigation key.': 'Pakai huruf, angka, tombol F, atau tombol navigasi.',
  'Shortcut unavailable. Record another combination.': 'Pintasan tidak tersedia. Rekam kombinasi lain.', 'Could not load preferences. Reopen Settings to retry.': 'Gagal memuat pengaturan. Buka ulang Settings.',
  // AI Agent
  'AI Agent': 'AI Agent', 'Alerts': 'Peringatan', 'Sounds': 'Suara', 'Weekly recap': 'Rekap mingguan', 'Plan usage': 'Pemakaian paket',
  'Connect': 'Hubungkan', 'Disconnect': 'Putuskan', 'Then run /hooks once': 'Lalu jalankan /hooks sekali', 'View only': 'Hanya lihat', 'Restart it once': 'Restart sekali',
  'View only. Restart it once': 'Hanya lihat. Restart sekali', 'View only. Run hermes plugins enable sora': 'Hanya lihat. Jalankan hermes plugins enable sora',
  'More options': 'Opsi lain', 'Approve Antigravity commands': 'Setujui perintah Antigravity', 'Reconnect Antigravity after changing': 'Hubungkan ulang Antigravity setelah diubah',
  'Sounds folder': 'Folder suara', 'Your own finish.wav, approval.wav, question.wav': 'finish.wav, approval.wav, question.wav milikmu', 'Open': 'Buka',
  'Other agents': 'Agent lain', 'Sends each event as JSON on stdin': 'Mengirim tiap event sebagai JSON lewat stdin', 'Copy command': 'Salin perintah',
  'What each agent can do': 'Kemampuan tiap agent', 'Agent': 'Agent', 'Activity': 'Aktivitas', 'Approval': 'Persetujuan', 'Questions': 'Pertanyaan', 'Changes': 'Perubahan',
  "Copied. Replace my-agent with the tool's name.": 'Tersalin. Ganti my-agent dengan nama tool-nya.',
  'File': 'File', 'Backup': 'Cadangan', 'Cancel': 'Batal', 'Install': 'Pasang', 'Remove': 'Hapus', 'None (the file is new)': 'Tidak ada (file baru)', 'No change.': 'Tidak ada perubahan.',
  // Meeting
  'Meeting': 'Meeting', 'Meeting recorder': 'Perekam meeting', 'Tell people when you record.': 'Beri tahu peserta saat kamu merekam.', 'Recording': 'Rekaman',
  'Save to': 'Simpan ke', 'Change': 'Ubah', 'Transcript language': 'Bahasa transkrip', 'Picking one is more accurate than Auto.': 'Pilih satu agar lebih akurat.',
  'Indonesian': 'Indonesia', 'English': 'Inggris', 'Transcript': 'Transkrip', 'Summary': 'Ringkasan', 'Optional': 'Opsional',
  'Lite': 'Ringan', 'Balanced': 'Seimbang', 'Powerful': 'Kuat', 'Short': 'Singkat', 'Standard': 'Standar', 'Deep': 'Mendalam',
  // package details from src/local_ai.rs
  'Fast, 4 GB RAM': 'Cepat, RAM 4 GB', 'Accurate, 8 GB RAM': 'Akurat, RAM 8 GB', 'Most accurate, 16 GB RAM': 'Paling akurat, RAM 16 GB',
  'Key points only': 'Poin penting saja', 'Topics and action items': 'Topik dan tindak lanjut', 'Detailed, for long meetings': 'Detail, untuk meeting panjang', 'Tells speakers apart': 'Membedakan pembicara',
  'Recommended': 'Disarankan',
  // Chat
  'Chat apps': 'Aplikasi chat', 'Other apps': 'Aplikasi lain', 'Notifications': 'Notifikasi',
  'WhatsApp replies': 'Balas WhatsApp', 'Answer WhatsApp messages from the notch.': 'Balas pesan WhatsApp dari notch.',
  'Unofficial WhatsApp.': 'WhatsApp tidak resmi.',
  ' Sora connects as a linked device. WhatsApp may limit or ban your account. Be careful!': ' Sora terhubung sebagai perangkat tertaut. WhatsApp bisa membatasi atau memblokir akunmu. Hati-hati!',
  'Account': 'Akun', 'Link': 'Tautkan', 'New code': 'Kode baru', 'Unlink': 'Lepas', 'QR code to link WhatsApp': 'Kode QR untuk menautkan WhatsApp',
  'On your phone open WhatsApp, then Settings, Linked devices, Link a device, and scan this code.': 'Di HP, buka WhatsApp, lalu Setelan, Perangkat tertaut, Tautkan perangkat, dan pindai kode ini.',
  'Not linked': 'Belum tertaut', 'Connecting…': 'Menghubungkan…', 'Scan the code below': 'Pindai kode di bawah', 'Linked': 'Tertaut', 'Linked, offline': 'Tertaut, offline',
  'Not connected': 'Tidak terhubung', 'Offline': 'Offline', 'Helper missing': 'Helper tidak ada', 'Linking': 'Menautkan',
  'Telegram replies': 'Balas Telegram', 'Answer Telegram messages from the notch.': 'Balas pesan Telegram dari notch.',
  'Not signed in': 'Belum masuk', 'Sign out': 'Keluar', 'Phone number': 'Nomor HP', 'Code': 'Kode', 'Two-step password': 'Kata sandi dua langkah', 'Next': 'Lanjut',
  'This build has no Telegram app key': 'Build ini tidak punya kunci aplikasi Telegram', 'Starting…': 'Memulai…', 'Waiting for the code': 'Menunggu kode',
  'Waiting for your password': 'Menunggu kata sandi', 'Signed in': 'Sudah masuk', 'Signing out…': 'Keluar…', 'Not set up': 'Belum disiapkan',
  'Notification access': 'Akses notifikasi', 'Needed to show other chat apps': 'Dibutuhkan untuk aplikasi chat lain',
  'Discord, Instagram, Messenger, LINE, Signal, Slack and more show from their Windows notifications. A click opens the chat in its own app.': 'Discord, Instagram, Messenger, LINE, Signal, Slack, dan lainnya tampil dari notifikasi Windows. Klik untuk membuka chat di aplikasinya.',
  // What's New
  'New Settings': 'Settings baru', 'A cleaner layout, an Advanced section, and English or Indonesian.': 'Tampilan lebih rapi, bagian Lanjutan, dan pilihan bahasa Inggris atau Indonesia.',
  'Chat in the notch': 'Chat di notch', 'Chat apps show messages in the notch. Click one to open that chat.': 'Pesan dari aplikasi chat tampil di notch. Klik untuk membuka chat itu.',
  'Reminders from the notch': 'Pengingat dari notch', 'Timer and calendar reminders grow out of the notch.': 'Pengingat timer dan kalender muncul dari notch.',
  'Calmer floating notch': 'Notch melayang lebih tenang', 'Other windows never change it. A click opens it.': 'Jendela lain tidak mengubahnya. Klik untuk membuka.',
  // errors made in this page
  'Run Sora.exe to change settings': 'Jalankan Sora.exe untuk mengubah pengaturan', 'Something went wrong.': 'Terjadi kesalahan.',
};
// Text with a value inside: [English pattern, Indonesian with $1].
const PATTERNS = [
  [/^Version (.+) is ready\.$/, 'Versi $1 siap dipasang.'], [/^Update to v(.*)$/, 'Perbarui ke v$1'], [/^Downloading (\d+)%$/, 'Mengunduh $1%'],
  [/^Display (\d+)$/, 'Layar $1'], [/^Install hooks for (.+)$/, 'Pasang hook untuk $1'], [/^Remove hooks from (.+)$/, 'Hapus hook dari $1'],
  [/^Linked as (.+)$/, 'Tertaut sebagai $1'], [/^Linked, offline as (.+)$/, 'Tertaut, offline sebagai $1'], [/^Signed in as (.+)$/, 'Masuk sebagai $1'],
];
const ATTRS = ['placeholder', 'aria-label'];
let lang = 'en';
try { lang = localStorage.getItem(KEY) === 'id' ? 'id' : 'en'; } catch {}

export const getLang = () => lang;
export function t(en) {
  if (lang !== 'id' || !en) return en;
  if (Object.hasOwn(ID, en)) return ID[en];
  for (const [re, out] of PATTERNS) if (re.test(en)) return en.replace(re, out);
  return en;
}

// Per node: the English it was written with and what we last showed. A value that differs from what we showed was
// set by the page code, so it is the new English.
const textEn = new WeakMap(), attrEn = new WeakMap();
const skip = n => !!n.parentElement?.closest('script,style,code,pre,[data-no-i18n]');
function doText(node) {
  if (skip(node)) return;
  const shown = node.nodeValue, seen = textEn.get(node);
  const en = seen && seen.shown === shown ? seen.en : shown, want = t(en);
  textEn.set(node, { en, shown: want });
  if (shown !== want) node.nodeValue = want;
}
function doAttrs(el) {
  let saved = attrEn.get(el); if (!saved) attrEn.set(el, saved = {});
  for (const a of ATTRS) {
    if (!el.hasAttribute(a)) continue;
    const shown = el.getAttribute(a), seen = saved[a];
    const en = seen && seen.shown === shown ? seen.en : shown, want = t(en);
    saved[a] = { en, shown: want };
    if (shown !== want) el.setAttribute(a, want);
  }
}
function walk(root) {
  if (root.nodeType === 3) return doText(root);
  if (root.nodeType !== 1) return;
  doAttrs(root);
  for (const el of root.querySelectorAll('[placeholder],[aria-label]')) doAttrs(el);
  const it = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = it.nextNode(); n; n = it.nextNode()) if (n.nodeValue.trim()) doText(n);
}
const watcher = new MutationObserver(records => {
  for (const r of records) {
    if (r.type === 'characterData') doText(r.target);
    else if (r.type === 'attributes') doAttrs(r.target);
    else r.addedNodes.forEach(walk);
  }
  watcher.takeRecords(); // our own edits above
});

export function setLang(next) {
  lang = next === 'id' ? 'id' : 'en';
  try { localStorage.setItem(KEY, lang); } catch {}
  document.documentElement.lang = lang;
  walk(document.body); watcher.takeRecords();
}
export function startI18n() {
  document.documentElement.lang = lang;
  walk(document.body);
  watcher.observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ATTRS });
}
