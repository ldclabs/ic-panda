# Dasar Privasi dMsg

Kemas kini terakhir: 2026-10-29.

dMsg menyulitkan kandungan peribadi pada peranti anda yang dibenarkan. Dasar ini meliputi dmsg.net, aplikasi web warisan (legacy) baca sahaja, aplikasi desktop dan iOS, connect.dmsg.net, perkhidmatan awan serta canister pada Internet Computer (ICP). “Kami” bermaksud pasukan yang mengendalikan dMsg di bawah tadbir urus ICPanda DAO. Ketersediaan ciri individu bergantung pada versi aplikasi dan konfigurasi perkhidmatan.

Kami tidak menjual data peribadi atau menggunakan kandungan anda untuk tujuan pengiklanan. Laman web ini tidak mengandungi penjejak pengiklanan atau analitik.

## 1. Kandungan dan maklumat awam

Item peti besi (Vault), mesej peribadi dan fail disulitkan sebelum dimuat naik. Kunci kandungan kekal pada peranti yang dibenarkan atau di dalam sampul pemulihan/peranti yang tidak dapat dibuka oleh perkhidmatan. Penerima boleh membaca dan menyimpan salinan yang dikongsi. Profil awam, avatar, handle (nama pengguna awam), dokumen direktori dan maklumat yang anda sengaja terbitkan adalah bersifat awam. Laporan penyalahgunaan ialah pendedahan berasingan: moderator hanya menerima sebab, nota, pengecam akaun/sumber dan teks mesej yang anda pilih secara eksplisit untuk disertakan. Membuat laporan tidak memberikan kunci akaun anda kepada moderator.

## 2. Metadata perkhidmatan

Perkhidmatan ini memproses pengecam akaun dan log masuk, kunci awam peranti, keahlian saluran, peraturan kenalan dan sekatan, saiz objek, masa permintaan, alamat IP dan maklumat permintaan operasi. Status akaun diperakui, resit tandatangan dan kelayakan (entitlement) membolehkan aplikasi klien dan perkhidmatan mengesahkan kebenaran. Transaksi lejar serta rekod dalam rantaian (on-chain) yang awam atau diperakui secara amnya tidak boleh dipadamkan.

Jika anda menghidupkan pemberitahuan, perkhidmatan pemberitahuan turut memproses perkara yang diperlukan untuk menapis amaran sebelum dihantar: pengecam sumber legap, saluran yang anda senyapkan atau sembunyikan, waktu senyap dan zon waktu anda, resit tontonan (bila anda benar-benar melihat peringatan dan sehingga mesej mana), serta akaun yang didaftarkan pada pemasangan yang sama. Untuk peringatan tarikh akhir bagi pemulihan akaun yang belum selesai, peranti anda memuat naik rekod pemulihan, termasuk identiti log masuk baharu dan kunci awam peranti; perkhidmatan mengesahkannya dan hanya menyimpan peringatan serta masa pelaksanaannya. Perkhidmatan tidak menerima nama saluran, kandungan mesej atau kunci untuk menyahsulitnya. Membatalkan pendaftaran, mengalih keluar akaun daripada peranti atau memadamkan akaun perkhidmatan akan mengosongkan pilihan, resit tontonan dan pautan pemasangan ini; rekod penghantaran dan cubaan semula hanya disimpan sehingga tamat tempoh.

## 3. Perkhidmatan Apple dan pembayaran

Pada iOS, pemberitahuan pilihan menggunakan perkhidmatan Apple Push Notification (APNs). Apple dan perkhidmatan penghantaran kami memproses pengecam pemasangan, token peranti dan metadata penghantaran. Pemberitahuan hanya mengandungi peringatan umum dan laluan legap, bukannya isi mesej, kata laluan atau nama fail. Anda boleh mematikan pemberitahuan dalam dMsg atau tetapan sistem; pemberitahuan yang telah dihantar tidak semestinya dapat ditarik balik. Jika beberapa akaun menerima pemberitahuan pada satu pemasangan yang sama, token peranti yang dikongsi membolehkan Apple dan perkhidmatan penghantaran kami mengaitkannya; dMsg meminta persetujuan anda sebelum menghidupkan pemberitahuan untuk akaun lain.

App Attest memproses pengecam kunci yang dijana aplikasi, bukti pengesahan/penegasan (attestation/assertion) dan cabaran kriptografi (challenge) untuk mengesahkan pemasangan aplikasi. Face ID, Touch ID dan kod laluan peranti dikendalikan oleh sistem pengendalian; dMsg tidak menerima imej biometrik atau kod laluan peranti anda.

Pembelian StoreKit dikendalikan oleh Apple. Pengesah dan canister perdagangan (commerce canister) kami memproses pengecam transaksi/produk, token pengikat akaun, tarikh pembelian dan tamat tempoh serta status bayaran balik/pembatalan. Ia tidak menerima butiran kad pembayaran anda. Klien dMsg lain mungkin menyokong pembayaran dompet ICP dan keahlian PANDA, dengan rekod lejar dan kelayakan awam. Pemadaman akaun dMsg tidak membatalkan langganan Apple; sila uruskan langganan dalam Akaun Apple anda.

Dasar dan cadangan dompet berbilang tandatangan (multisig) diproses oleh canister dompet; keterlihatannya mengikut tetapan dompet tersebut. Pembekal RPC blokrantaian menerima alamat, data transaksi dan maklumat IP yang diperlukan untuk pertanyaan baki, simulasi yuran dan penyiaran (broadcasting) transaksi. Titik akhir lalai termasuk mempool.space, PublicNode, mainnet.base.org dan api.mainnet-beta.solana.com; alternatif yang dikonfigurasikan sendiri mempunyai amalan privasi mereka tersendiri.

## 4. Data setempat dan penyepaduan sistem

Aplikasi asli (native) menyimpan ruang kerja tersulit setempat, kunci yang dilindungi oleh sistem pengendalian (jika disokong), draf, operasi yang belum selesai dan pilihan tetapan. Membuka kunci menjadikan kandungan terpilih tersedia dalam memori aplikasi. Fail yang dieksport dan teks yang disalin mungkin disimpan oleh aplikasi destinasi.

Sambungan Kongsi (Share) iOS hanya menyediakan sementara item yang anda pilih untuk diimport ke dalam akaun kemudian. Penyepaduan Isi Auto Kata Laluan (AutoFill) dan aplikasi Fail hanya mengandungi syot kilat (snapshot) yang dipilih secara eksplisit dan disulitkan secara berasingan. Isi Auto memerlukan pengesahan sistem yang bebas; syot kilat tamat tempoh selepas 24 jam. Sesi akses aplikasi Fail berlangsung paling lama lima minit dan tamat apabila dMsg dikunci, tetapi aplikasi Fail dan penerima lain mungkin mengekalkan salinan yang telah dimuat turun. Menukar item sumber tidak mengemas kini syot kilat yang telah dieksport secara automatik.

Pemindahan latar belakang yang dimulakan secara eksplisit hanya menyimpan teks sifir terhad dan kuasa pemindahan di dalam direktori peranti berasingan yang boleh diakses selepas peranti dibuka kunci buat kali pertama. Ia tidak memegang kunci tandatangan akaun atau kunci kandungan. Akaun asal mesti dibuka kunci untuk mengesahkan dan melengkapkan pemindahan. Sistem pengendalian mungkin menangguhkan atau menghentikan penghantaran.

Laman web mengingati pilihan bahasa anda secara setempat. Aplikasi web warisan menggunakan storan pelayar untuk sesi dan data cache; paparan dompetnya mungkin meminta harga daripada ICPSwap. Mengalih keluar aplikasi atau mengosongkan datanya boleh menyebabkan kehilangan kekal kandungan yang tidak pernah disegerakkan atau dieksport.

## 5. Pembekal perkhidmatan dan pengekalan data

Cloudflare memproses teks sifir (ciphertext) awan dan metadata permintaan. Pembekal nod dan gerbang ICP mengendalikan canister dan laman web. Apple mengedarkan aplikasi iOS dan menyediakan perkhidmatan pilihan yang diterangkan di atas. Internet Identity, pembekal dompet dan aplikasi bersambung mempunyai dasar privasi mereka sendiri. Kami boleh mendedahkan maklumat yang kami pegang apabila dikehendaki oleh undang-undang, termasuk laporan yang dikemukakan secara eksplisit; kami tidak dapat mendedahkan teks biasa (plaintext) yang tidak kami miliki.

Teks sifir awan mengikut peraturan penyimpanan dan pemadaman akaun. Pemadaman akaun perkhidmatan ialah permintaan pembersihan yang dijejaki: ia menyekat akses perkhidmatan baharu, mengalih keluar kandungan awan milik akaun dan salinan avatar awam, serta hanya melaporkan penyelesaian selepas pembersihan selesai. Ia tidak memadamkan salinan yang dikongsi dengan orang lain, salinan setempat pada peranti lain, sejarah lejar awam atau rekod kewangan/anti-ulang tayang (anti-replay) yang diperlukan. Transaksi yang belum selesai atau saluran aktif yang dimiliki mungkin perlu diselesaikan terlebih dahulu. Rekod laporan disimpan sehingga 90 hari untuk semakan; token penghantaran tamat tempoh melainkan diperbaharui. Log operasi disimpan hanya seperti yang diperlukan untuk keselamatan, pencegahan penyalahgunaan dan penyelesaian masalah.

## 6. Pilihan, hak dan hubungan

Anda boleh mengeksport atau memadamkan kandungan, membatalkan kebenaran peranti atau pelayar, menyekat pengirim, melaporkan penyalahgunaan, menukar peraturan kenalan, meminta pemadaman akaun perkhidmatan dan mengalih keluar data akaun peranti ini secara berasingan. Bergantung pada lokasi anda, anda mungkin mempunyai hak untuk mengakses, membetulkan, memadam atau membantah pemprosesan maklumat peribadi. Hubungi [Sokongan](https://dmsg.net/_/support) untuk sebarang permintaan atau laporan kerentanan keselamatan secara peribadi. Kami akan menerangkan apabila penyulitan, rekod transaksi yang disimpan atau rekod awam yang tidak boleh diubah mengehadkan sesuatu permintaan.

Tiada sistem yang selamat secara mutlak. Kemas kini aplikasi dan penaiktarafan canister di bawah tadbir urus DAO kekal sebagai titik kepercayaan. dMsg tidak ditujukan untuk kanak-kanak di bawah umur 13 tahun atau di bawah umur minimum yang terpakai untuk menggunakan perkhidmatan dalam talian tanpa kebenaran ibu bapa. Kami akan mengemas kini tarikh di atas dan mengumumkan perubahan dasar yang penting dalam aplikasi atau di laman web.
