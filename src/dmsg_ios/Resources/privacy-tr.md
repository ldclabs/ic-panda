# dMsg Gizlilik Politikası

Son güncelleme: 2026-10-29.

dMsg, yetkili cihazlarınızdaki özel içerikleri şifreler. Bu politika; dmsg.net web sitesini, salt okunur eski web uygulamasını, masaüstü ve iOS uygulamalarını, connect.dmsg.net adresini, bulut hizmetini ve Internet Computer (ICP) üzerindeki canister'ları kapsar. “Biz”, ICPanda DAO yönetişimi altında dMsg'yi işleten ekibi ifade eder. Bireysel özelliklerin kullanılabilirliği, uygulama sürümüne ve hizmet yapılandırmasına bağlıdır.

Kişisel verileri satmıyoruz ve içeriğinizi reklam amacıyla kullanmıyoruz. Web sitesinde hiçbir reklam veya analiz izleyicisi (tracker) bulunmaz.

## 1. İçerik ve herkese açık bilgiler

Kasa (Vault) öğeleri, özel mesajlar ve dosyalar yüklenmeden önce şifrelenir. İçerik anahtarları yalnızca yetkili cihazlarda veya hizmetin açamadığı kurtarma/cihaz zarflarında kalır. Alıcılar paylaşılan kopyaları okuyabilir ve saklayabilir. Herkese açık profiller, avatarlar, kullanıcı adları (handle), dizin belgeleri ve bilerek yayınladığınız bilgiler herkese açıktır. Kötüye kullanım bildirimi (raporlama) ayrı bir açıklamadır: moderatörler yalnızca bildirim nedeninizi, notunuzu, hesap/kaynak tanımlayıcılarınızı ve açıkça eklemeyi seçtiğiniz mesaj metnini alır. Bildirimde bulunmak, moderatörlere hesabınızın anahtarlarını vermez.

## 2. Hizmet meta verileri

Hizmet; hesap ve giriş tanımlayıcılarını, cihaz açık anahtarlarını, kanal üyeliklerini, iletişim ve engelleme kurallarını, nesne boyutlarını, istek zamanlamalarını, IP adreslerini ve operasyonel istek bilgilerini işler. Sertifikalı hesap durumu, imza makbuzları ve kullanım hakları (entitlement), istemci uygulamaların ve hizmetlerin yetki doğrulaması yapmasına olanak tanır. Defter (ledger) işlemleri ile herkese açık veya sertifikalı zincir üstü (on-chain) kayıtlar genel olarak silinemez.

Bildirimleri açarsanız bildirim hizmeti, uyarıları gönderilmeden önce filtrelemek için gereken verileri de işler: opak kaynak tanımlayıcıları, sessize aldığınız veya gizlediğiniz kanallar, sessiz saatleriniz ve saat diliminiz, görüntüleme bilgileri (bir hatırlatıcıyı gerçekten ne zaman görüntülediğiniz ve hangi mesaja kadar okuduğunuz) ve aynı kurulumda hangi hesapların kayıtlı olduğu. Bekleyen bir hesap kurtarma işleminin süre hatırlatıcıları için cihazınız, yeni giriş kimliğini ve cihaz açık anahtarını içeren kurtarma kaydını yükler; hizmet bunu doğrular ve yalnızca hatırlatıcıyı ve yürütme zamanını saklar. Hizmet; kanal adlarını, mesaj içeriğini veya bunların şifresini çözecek anahtarları almaz. Kaydı iptal etmek, hesabı cihazdan kaldırmak veya hizmet hesabını silmek bu tercihleri, görüntüleme bilgilerini ve kurulum bağlantılarını temizler; teslimat ve yeniden deneme kayıtları yalnızca süreleri dolana kadar tutulur.

## 3. Apple hizmetleri ve ödemeler

iOS'ta isteğe bağlı bildirimler Apple Anında Bildirim (Push Notification) hizmetini kullanır. Apple ve iletim hizmetimiz; kurulum tanımlayıcısını, cihaz belirtecini (token) ve iletim meta verilerini işler. Bildirimler mesaj gövdesi, parola veya dosya adı içermez; yalnızca genel bir hatırlatıcı ve opak bir yönlendirme bilgisi içerir. Bildirimleri dMsg içinden veya sistem ayarlarından kapatabilirsiniz; halihazırda iletilmiş bildirimler her zaman geri çekilemeyebilir. Aynı kurulumda birden fazla hesap bildirim alıyorsa paylaşılan cihaz belirteci, Apple ve iletim hizmetimizin bu hesapları ilişkilendirmesine olanak tanır; dMsg başka bir hesap için bildirimleri açmadan önce onayınızı ister.

App Attest, uygulama kurulumunun doğruluğunu teyit etmek için uygulama tarafından oluşturulan bir anahtar tanımlayıcısını, doğrulama/beyan (attestation/assertion) kanıtlarını ve sınamaları (challenge) işler. Face ID, Touch ID ve cihaz parolası işletim sistemi tarafından yönetilir; dMsg biyometrik görüntüleri veya cihaz parolanızı almaz.

StoreKit satın alımları Apple tarafından yürütülür. Doğrulayıcı hizmetimiz ve ticaret canister'ımız; işlem/ürün tanımlayıcılarını, hesap bağlama belirtecini, satın alma ve sona erme tarihlerini ve iade/iptal durumunu işler. Ödeme kartı bilgilerinizi almazlar. Diğer dMsg istemcileri, herkese açık defter ve hak kayıtlarıyla ICP cüzdan ödemelerini ve PANDA üyeliğini destekleyebilir. dMsg hesabının silinmesi Apple aboneliğinizi iptal etmez; aboneliklerinizi Apple Hesabınızdan yönetin.

Çoklu imza (multisig) cüzdan politikaları ve teklifleri cüzdan canister'ları tarafından işlenir; görünürlükleri cüzdanın kendi ayarlarını izler. Blokzincir RPC sağlayıcıları; bakiye sorguları, ücret simülasyonu ve işlem yayınlama (broadcasting) için gereken adresleri, işlem verilerini ve IP bilgilerini alır. Varsayılan uç noktalar arasında mempool.space, PublicNode, mainnet.base.org ve api.mainnet-beta.solana.com bulunur; yapılandırılan alternatif uç noktaların kendi gizlilik uygulamaları geçerlidir.

## 4. Yerel veriler ve sistem entegrasyonları

Yerel (native) uygulamalar; şifrelenmiş bir yerel çalışma alanını, desteklenen sistemlerde işletim sistemi tarafından korunan anahtarları, taslakları, bekleyen işlemleri ve tercihleri saklar. Kilidin açılması, seçilen içeriği uygulama belleğinde kullanılabilir hale getirir. Dışa aktarılan dosyalar ve kopyalanan metinler hedef uygulamalar tarafından saklanabilir.

iOS Paylaş (Share) uzantısı, yalnızca daha sonra hesaba aktarmak üzere seçtiğiniz öğeleri geçici olarak hazırlar. Parola Otomatik Doldurma (AutoFill) ve Dosyalar (Files) entegrasyonları yalnızca açıkça seçilen ve ayrı olarak şifrelenen anlık görüntüleri (snapshot) içerir. Otomatik Doldurma bağımsız sistem kimlik doğrulaması gerektirir; anlık görüntülerin süresi 24 saat sonra dolar. Dosyalar erişim oturumları en fazla beş dakika sürer ve dMsg kilitlendiğinde sona erer; ancak Dosyalar uygulaması ve diğer alıcılar önceden indirilmiş kopyaları saklayabilir. Kaynak öğelerin değiştirilmesi, dışa aktarılmış bir anlık görüntüyü otomatik olarak güncellemez.

Açıkça başlatılan arka plan aktarımları, cihazın ilk kilidi açıldıktan sonra erişilebilen ayrı bir cihaz dizininde yalnızca sınırlı şifreli metni ve aktarım yetkisini saklar. Hesap imzalama veya içerik anahtarlarını barındırmaz. Bir aktarımı doğrulamak ve tamamlamak için orijinal hesabın kilidinin açılması gerekir. İşletim sistemi iletimi geciktirebilir veya durdurabilir.

Web sitesi dil tercihinizi yerel olarak hatırlar. Eski web uygulaması, oturumlar ve önbelleğe alınan veriler için tarayıcı depolama alanını kullanır; cüzdan görünümü ICPSwap'ten fiyat bilgisi isteyebilir. Bir uygulamayı kaldırmak veya verilerini temizlemek, hiç senkronize edilmemiş veya dışa aktarılmamış içeriklerin kalıcı olarak kaybolmasına yol açabilir.

## 5. Sağlayıcılar ve veri saklama

Cloudflare, bulut şifreli metinlerini ve istek meta verilerini işler. ICP düğüm (node) sağlayıcıları ve ağ geçitleri canister'ları ve web sitelerini çalıştırır. Apple, iOS uygulamasını dağıtır ve yukarıda açıklanan isteğe bağlı hizmetleri sunar. Internet Identity, cüzdan sağlayıcıları ve bağlı uygulamalar kendi gizlilik politikalarına tabidir. Açıkça gönderilen bildirimler de dahil olmak üzere, yasal olarak zorunlu olduğunda elimizdeki bilgileri açıklayabiliriz; sahip olmadığımız düz metinleri (plaintext) ise açıklamamız mümkün değildir.

Buluttaki şifreli metinler hesabın depolama ve silme kurallarına tabidir. Hizmet hesabının silinmesi, ilerlemesi takip edilen bir temizleme isteğidir: yeni hizmet erişimini engeller, hesaba ait bulut içeriğini ve herkese açık avatar kopyalarını kaldırır ve yalnızca temizleme tamamlandıktan sonra işlemin bittiğini bildirir. Başkalarındaki paylaşılan kopyaları, diğer cihazlardaki yerel kopyaları, herkese açık defter geçmişini veya zorunlu finansal/tekrar oynatma önleme (anti-replay) kayıtlarını silmez. Sonuçlanmamış işlemlerin veya sahip olunan aktif kanalların önce çözümlenmesi gerekebilir. Kötüye kullanım bildirim kayıtları inceleme amacıyla 90 güne kadar saklanır; iletim belirteçlerinin süresi yenilenmediği takdirde dolar. Operasyonel günlükler yalnızca güvenlik, kötüye kullanımın önlenmesi ve sorun giderme için gerektiği sürece tutulur.

## 6. Seçenekler, haklar ve iletişim

İçeriği dışa aktarabilir veya silebilir, cihazların veya tarayıcıların yetkisini kaldırabilir, gönderenleri engelleyebilir, kötüye kullanımı bildirebilir, iletişim kurallarını değiştirebilir, hizmet hesabının silinmesini talep edebilir ve bu cihazdaki hesap verilerini ayrı olarak kaldırabilirsiniz. Bulunduğunuz yere bağlı olarak kişisel bilgilere erişme, bunları düzeltme, silme veya işlenmesine itiraz etme haklarına sahip olabilirsiniz. Talepleriniz veya gizli güvenlik açığı bildirimleriniz için [Destek](https://dmsg.net/_/support) ile iletişime geçin. Şifrelemenin, saklanması gereken işlem kayıtlarının veya değiştirilemez kamuya açık kayıtların bir talebi hangi durumlarda sınırlandırdığını açıklarız.

Hiçbir sistem kusursuz derecede güvenli değildir. Uygulama güncellemeleri ve DAO yönetişimi altındaki canister yükseltmeleri güvene dayalı noktalar olmaya devam eder. dMsg, 13 yaşın altındaki veya ebeveyn izni olmadan çevrimiçi hizmetleri kullanmak için geçerli asgari yaşın altındaki çocuklara yönelik değildir. Önemli politika değişikliklerinde yukarıdaki tarihi günceller ve değişiklikleri uygulamada veya web sitesinde duyururuz.
