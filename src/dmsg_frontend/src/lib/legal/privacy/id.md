# Kebijakan Privasi dMsg

Terakhir diperbarui: 27-10-2026.

dMsg mengenkripsi konten pribadi di perangkat resmi Anda. Kebijakan ini mencakup dmsg.net, aplikasi web lama yang hanya dapat dibaca, aplikasi desktop dan iOS, connect.dmsg.net, layanan cloud, dan Komputer Internet (ICP) canisters. “Kami” berarti tim yang mengoperasikan dMsg di bawah tata kelola ICPanda DAO. Ketersediaan fitur individual bergantung pada versi aplikasi dan konfigurasi layanan.

Kami tidak menjual data pribadi atau menggunakan konten Anda untuk iklan. Situs web ini tidak berisi pelacak iklan atau analitik.

## 1. Konten dan informasi publik

Item brankas, pesan pribadi, dan file dienkripsi sebelum diunggah. Kunci konten tetap berada di perangkat resmi atau dalam amplop pemulihan/perangkat yang tidak dapat dibuka oleh layanan. Penerima dapat membaca dan menyimpan salinan bersama. Profil publik, avatar, nama pengguna, dokumen direktori, dan informasi yang sengaja Anda terbitkan bersifat publik. Laporan merupakan pengungkapan terpisah: moderator menerima alasan, catatan, pengidentifikasi akun/sumber daya, dan hanya teks pesan yang secara eksplisit Anda pilih untuk disertakan. Pelaporan tidak memberikan moderator kunci akun Anda.

## 2. Metadata layanan

Layanan ini memproses pengidentifikasi akun dan login, kunci publik perangkat, keanggotaan saluran, aturan kontak dan pemblokiran, ukuran objek, waktu permintaan, alamat IP, dan informasi permintaan operasional. Status akun tersertifikasi, tanda terima tanda tangan, dan hak memungkinkan klien dan layanan memverifikasi otoritas. Transaksi buku besar dan catatan on-chain publik atau tersertifikasi umumnya tidak dapat dihapus.

## 3. Layanan dan pembayaran Apple

Di iOS, notifikasi opsional menggunakan layanan Notifikasi Apple Push. Apple dan layanan pengiriman kami memproses pengidentifikasi instalasi, token perangkat, dan metadata pengiriman. Pemberitahuan berisi pengingat umum dan rute buram, bukan isi pesan, kata sandi, atau nama file. Anda dapat menonaktifkan notifikasi di dMsg atau pengaturan sistem; pemberitahuan yang sudah terkirim tidak selalu dapat ditarik kembali.

App Attest memproses pengidentifikasi kunci, bukti pengesahan/pernyataan, dan tantangan yang dihasilkan aplikasi untuk memverifikasi instalasi aplikasi. Face ID, Touch ID dan kode sandi perangkat ditangani oleh sistem operasi; dMsg tidak menerima gambar biometrik atau kode sandi Anda.

Pembelian StoreKit ditangani oleh Apple. Verifikator dan perdagangan kami canister memproses pengidentifikasi transaksi/produk, token pengikatan akun, tanggal pembelian dan kedaluwarsa, dan status pengembalian dana/pencabutan. Mereka tidak menerima rincian kartu pembayaran Anda. Klien dMsg lainnya dapat mendukung pembayaran dompet ICP dan keanggotaan PANDA, dengan buku besar umum dan catatan hak. Penghapusan akun tidak membatalkan langganan Apple; mengelola langganan di akun Apple Anda.

Kebijakan dan proposal dompet tim diproses oleh dompet canisters; visibilitasnya mengikuti pengaturan dompet. Penyedia Blockchain RPC menerima alamat, data transaksi, dan informasi IP yang diperlukan untuk pertanyaan saldo, simulasi biaya, dan penyiaran. Titik akhir default mencakup mempool.space, PublicNode, mainnet.base.org, dan api.mainnet-beta.solana.com; alternatif yang dikonfigurasi memiliki praktik privasinya sendiri.

## 4. Integrasi data dan sistem lokal

Aplikasi asli menyimpan ruang kerja terenkripsi lokal, kunci yang dilindungi oleh sistem operasi jika didukung, konsep, operasi yang tertunda, dan preferensi. Membuka kunci membuat konten yang dipilih tersedia di memori aplikasi. File yang diekspor dan teks yang disalin mungkin disimpan oleh aplikasi tujuannya.

Ekstensi iOS Share hanya menampilkan item yang Anda pilih untuk impor akun berikutnya. Integrasi IsiOtomatis Kata Sandi dan File hanya berisi snapshot yang dipilih secara eksplisit dan dienkripsi secara terpisah. IsiOtomatis memerlukan otentikasi sistem independen; snapshot kedaluwarsa setelah 24 jam. Sesi akses file berlangsung paling lama lima menit dan berakhir ketika dMsg terkunci, namun aplikasi File dan penerima lainnya mungkin menyimpan salinan yang sudah diunduh. Mengubah item sumber tidak memperbarui snapshot yang diekspor secara otomatis.

Transfer latar belakang eksplisit hanya menyimpan teks sandi terbatas dan otoritas transfer dalam direktori perangkat terpisah yang dapat diakses setelah pembukaan kunci perangkat pertama. Mereka tidak memegang kunci penandatanganan akun atau konten. Akun asli harus dibuka kuncinya untuk memverifikasi dan menyelesaikan transfer. Sistem mungkin menunda atau menghentikan pengiriman.

Situs web mengingat bahasa Anda secara lokal. Aplikasi lama menggunakan penyimpanan browser untuk sesi dan data cache; tampilan dompetnya dapat meminta harga dari ICPSwap. Menghapus aplikasi atau menghapus datanya dapat menghilangkan konten yang tidak pernah disinkronkan atau diekspor secara permanen.

## 5. Penyedia dan retensi

Cloudflare memproses ciphertext cloud dan meminta metadata. Penyedia node dan gateway ICP mengoperasikan canisters dan situs web. Apple mendistribusikan aplikasi iOS dan menyediakan layanan opsional yang dijelaskan di atas. Internet Identity, penyedia dompet dan aplikasi yang terhubung memiliki kebijakan privasinya sendiri. Kami dapat mengungkapkan informasi yang kami miliki bila diwajibkan secara hukum, termasuk laporan yang disampaikan secara eksplisit; kami tidak dapat mengungkapkan teks biasa yang tidak kami miliki.

Cloud ciphertext mengikuti aturan penyimpanan dan penghapusan akun. Penghapusan akun layanan adalah permintaan pembersihan terlacak: penghapusan ini memblokir akses layanan baru, menghapus konten cloud milik akun dan salinan avatar publik, dan melaporkan penyelesaian hanya setelah pembersihan. Itu tidak menghapus salinan bersama orang lain, salinan lokal di perangkat lain, riwayat buku besar publik, atau catatan keuangan/anti-pemutaran ulang yang diperlukan. Transaksi yang belum diselesaikan atau saluran aktif yang dimiliki mungkin memerlukan penyelesaian terlebih dahulu. Catatan laporan disimpan hingga 90 hari untuk ditinjau; token pengiriman kedaluwarsa kecuali diperbarui. Log operasional disimpan hanya jika diperlukan untuk keamanan, pencegahan penyalahgunaan, dan pemecahan masalah.

## 6. Pilihan, hak dan kontak

Anda dapat mengekspor atau menghapus konten, mencabut perangkat atau browser, memblokir pengirim, melaporkan penyalahgunaan, mengubah aturan kontak, meminta penghapusan akun layanan, dan menghapus data akun perangkat ini secara terpisah. Tergantung pada lokasi Anda, Anda mungkin memiliki hak untuk mengakses, memperbaiki, menghapus, atau menolak pemrosesan informasi pribadi. Hubungi [Dukungan](https://dmsg.net/_/support) untuk permintaan atau laporan kerentanan pribadi. Kami menjelaskan kapan enkripsi, catatan transaksi yang disimpan, atau catatan publik yang tidak dapat diubah membatasi permintaan.

Tidak ada sistem yang benar-benar aman. Pembaruan aplikasi dan peningkatan canister di bawah tata kelola DAO tetap menjadi poin kepercayaan. dMsg tidak ditujukan untuk anak-anak di bawah 13 tahun atau di bawah usia minimum yang berlaku untuk menggunakan layanan online tanpa izin orang tua. Kami memperbarui tanggal di atas dan mengumumkan perubahan kebijakan material di aplikasi atau situs web.
