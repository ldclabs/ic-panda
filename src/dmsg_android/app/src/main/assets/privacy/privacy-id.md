# Kebijakan Privasi dMsg

Terakhir diperbarui: 2026-10-29.

dMsg mengenkripsi konten pribadi pada perangkat resmi Anda. Kebijakan ini mencakup dmsg.net, aplikasi web versi lama (legacy) yang bersifat hanya-baca, aplikasi desktop dan iOS, connect.dmsg.net, layanan cloud, serta canister pada Internet Computer (ICP). “Kami” berarti tim yang mengoperasikan dMsg di bawah tata kelola ICPanda DAO. Ketersediaan masing-masing fitur bergantung pada versi aplikasi dan konfigurasi layanan.

Kami tidak menjual data pribadi atau menggunakan konten Anda untuk iklan. Situs web ini tidak memuat pelacak iklan maupun analitik.

## 1. Konten dan informasi publik

Item brankas (Vault), pesan pribadi, dan file dienkripsi sebelum diunggah. Kunci konten tetap berada di perangkat resmi Anda atau di dalam amplop pemulihan/perangkat yang tidak dapat dibuka oleh layanan. Penerima dapat membaca dan menyimpan salinan yang dibagikan. Profil publik, avatar, handle (nama pengguna publik), dokumen direktori, dan informasi yang sengaja Anda terbitkan bersifat publik. Laporan penyalahgunaan merupakan pengungkapan terpisah: moderator hanya menerima alasan, catatan, pengidentifikasi akun/sumber daya, serta teks pesan yang secara eksplisit Anda pilih untuk disertakan. Melaporkan sesuatu tidak memberikan kunci akun Anda kepada moderator.

## 2. Metadata layanan

Layanan memproses pengidentifikasi akun dan login, kunci publik perangkat, keanggotaan saluran, aturan kontak dan pemblokiran, ukuran objek, waktu permintaan, alamat IP, serta informasi permintaan operasional. Status akun tersertifikasi, tanda terima tanda tangan, dan hak akses (entitlement) memungkinkan aplikasi klien dan layanan memverifikasi otoritas. Transaksi buku besar (ledger) serta catatan on-chain yang bersifat publik atau tersertifikasi umumnya tidak dapat dihapus.

Jika Anda mengaktifkan notifikasi, layanan notifikasi juga memproses hal yang diperlukan untuk menyaring peringatan sebelum dikirim: pengidentifikasi sumber daya yang buram (opaque), saluran yang Anda bisukan atau sembunyikan, jam tenang dan zona waktu, tanda terima tampilan (kapan Anda benar-benar melihat pengingat dan sampai pesan mana), serta akun mana yang terdaftar pada instalasi yang sama. Untuk pengingat tenggat pemulihan akun yang tertunda, perangkat Anda mengunggah catatan pemulihan, termasuk identitas login baru dan kunci publik perangkat; layanan memverifikasinya dan hanya menyimpan pengingat serta waktu pelaksanaannya. Layanan tidak menerima nama saluran, isi pesan, atau kunci untuk mendekripsinya. Mencabut pendaftaran, menghapus akun dari perangkat, atau menghapus akun layanan akan menghapus preferensi, tanda terima tampilan, dan tautan instalasi tersebut; catatan pengiriman dan percobaan ulang hanya disimpan sampai kedaluwarsa.

## 3. Layanan Apple dan pembayaran

Di iOS, notifikasi opsional menggunakan layanan Apple Push Notification (APNs). Apple dan layanan pengiriman kami memproses pengidentifikasi instalasi, token perangkat, dan metadata pengiriman. Notifikasi hanya berisi pengingat umum dan rute buram, bukan isi pesan, kata sandi, atau nama file. Anda dapat menonaktifkan notifikasi di dMsg atau melalui pengaturan sistem; notifikasi yang sudah terkirim tidak selalu dapat ditarik kembali. Jika beberapa akun menerima notifikasi pada satu instalasi yang sama, token perangkat bersama memungkinkan Apple dan layanan pengiriman kami mengaitkannya; dMsg akan meminta persetujuan Anda sebelum mengaktifkan notifikasi untuk akun lain.

App Attest memproses pengidentifikasi kunci yang dibuat aplikasi, bukti pengesahan/pernyataan (attestation/assertion), dan tantangan kriptografis (challenge) untuk memverifikasi instalasi aplikasi. Face ID, Touch ID, dan kode sandi perangkat ditangani langsung oleh sistem operasi; dMsg tidak menerima data biometrik maupun kode sandi perangkat Anda.

Pembelian StoreKit ditangani oleh Apple. Layanan verifikator dan canister niaga (commerce canister) kami memproses pengidentifikasi transaksi/produk, token pengikat akun, tanggal pembelian dan kedaluwarsa, serta status pengembalian dana/pencabutan. Keduanya tidak menerima rincian kartu pembayaran Anda. Klien dMsg lainnya dapat mendukung pembayaran dompet ICP dan keanggotaan PANDA, dengan catatan buku besar dan hak akses publik. Penghapusan akun dMsg tidak membatalkan langganan Apple; kelola langganan Anda melalui Akun Apple Anda.

Kebijakan dan proposal dompet multitanda tangan (multisig) diproses oleh canister dompet; visibilitasnya mengikuti pengaturan dompet tersebut. Penyedia RPC blockchain menerima alamat, data transaksi, dan informasi IP yang diperlukan untuk pengecekan saldo, simulasi biaya, dan penyiaran (broadcasting) transaksi. Endpoint default mencakup mempool.space, PublicNode, mainnet.base.org, dan api.mainnet-beta.solana.com; endpoint alternatif yang Anda konfigurasi sendiri memiliki kebijakan privasinya masing-masing.

## 4. Data lokal dan integrasi sistem

Aplikasi native menyimpan ruang kerja lokal terenkripsi, kunci yang dilindungi oleh sistem operasi (jika didukung), draf, operasi yang tertunda, dan preferensi. Membuka kunci aplikasi akan menyediakan konten terpilih di dalam memori aplikasi. File yang diekspor dan teks yang disalin dapat disimpan oleh aplikasi tujuan.

Ekstensi Bagikan (Share) iOS hanya menyiapkan sementara item yang Anda pilih untuk diimpor ke akun nantinya. Integrasi Isi Otomatis Kata Sandi (AutoFill) dan aplikasi File hanya berisi snapshot yang dipilih secara eksplisit dan dienkripsi secara terpisah. Isi Otomatis memerlukan autentikasi sistem tersendiri; snapshot kedaluwarsa setelah 24 jam. Sesi akses aplikasi File berlangsung paling lama lima menit dan berakhir saat dMsg terkunci, namun aplikasi File dan penerima lain mungkin tetap menyimpan salinan yang sudah diunduh. Mengubah item sumber tidak secara otomatis memperbarui snapshot yang telah diekspor.

Transfer latar belakang yang dimulai secara eksplisit hanya menyimpan teks tersandi terbatas dan otoritas transfer di dalam direktori perangkat terpisah yang dapat diakses setelah kunci perangkat dibuka pertama kali. Direktori tersebut tidak menyimpan kunci penandatanganan akun maupun kunci konten. Akun asli harus dibuka kuncinya untuk memverifikasi dan menyelesaikan transfer. Sistem operasi dapat menunda atau menghentikan pengiriman.

Situs web mengingat pilihan bahasa Anda secara lokal. Aplikasi web versi lama menggunakan penyimpanan browser untuk sesi dan data cache; tampilan dompetnya dapat meminta informasi harga dari ICPSwap. Menghapus aplikasi atau membersihkan datanya dapat menyebabkan hilangnya konten yang belum pernah disinkronkan atau diekspor secara permanen.

## 5. Penyedia layanan dan penyimpanan data

Cloudflare memproses teks tersandi (ciphertext) cloud dan metadata permintaan. Penyedia node dan gateway ICP mengoperasikan canister serta situs web. Apple mendistribusikan aplikasi iOS dan menyediakan layanan opsional yang dijelaskan di atas. Internet Identity, penyedia dompet, dan aplikasi yang terhubung memiliki kebijakan privasinya masing-masing. Kami dapat mengungkapkan informasi yang kami pegang apabila diwajibkan oleh hukum, termasuk laporan yang dikirimkan secara eksplisit; kami tidak dapat mengungkapkan teks biasa (plaintext) yang tidak kami miliki.

Teks tersandi di cloud mengikuti aturan penyimpanan dan penghapusan akun. Penghapusan akun layanan adalah permintaan pembersihan yang terlacak: memblokir akses layanan baru, menghapus konten cloud milik akun serta salinan avatar publik, dan hanya melaporkan penyelesaian setelah pembersihan tuntas. Tindakan ini tidak menghapus salinan yang telah dibagikan kepada orang lain, salinan lokal di perangkat lain, riwayat buku besar publik, atau catatan keuangan/anti-replay yang diperlukan. Transaksi yang belum selesai atau saluran aktif yang dimiliki mungkin perlu diselesaikan terlebih dahulu. Catatan laporan disimpan hingga 90 hari untuk ditinjau; token pengiriman akan kedaluwarsa kecuali diperbarui. Log operasional hanya disimpan selama diperlukan untuk keamanan, pencegahan penyalahgunaan, dan pemecahan masalah.

## 6. Pilihan, hak, dan kontak

Anda dapat mengekspor atau menghapus konten, mencabut akses perangkat atau browser, memblokir pengirim, melaporkan penyalahgunaan, mengubah aturan kontak, meminta penghapusan akun layanan, serta menghapus data akun pada perangkat ini secara terpisah. Bergantung pada lokasi Anda, Anda mungkin memiliki hak untuk mengakses, memperbaiki, menghapus, atau menolak pemrosesan informasi pribadi. Hubungi [Dukungan](https://dmsg.net/_/support) untuk mengajukan permintaan atau laporan kerentanan keamanan secara tertutup. Kami akan menjelaskan apabila enkripsi, catatan transaksi yang wajib disimpan, atau catatan publik yang tidak dapat diubah membatasi suatu permintaan.

Tidak ada sistem yang sepenuhnya aman. Pembaruan aplikasi dan peningkatan canister di bawah tata kelola DAO tetap menjadi titik kepercayaan. dMsg tidak ditujukan bagi anak-anak di bawah usia 13 tahun atau di bawah batas usia minimum yang berlaku untuk menggunakan layanan online tanpa persetujuan orang tua. Kami akan memperbarui tanggal di bagian atas dan mengumumkan perubahan kebijakan yang bersifat material melalui aplikasi atau situs web.
