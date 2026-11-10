# Chính sách quyền riêng tư của dMsg

Cập nhật lần cuối: 2026-10-29.

dMsg mã hóa nội dung riêng tư trên các thiết bị được ủy quyền của bạn. Chính sách này áp dụng cho dmsg.net, ứng dụng web bản cũ chỉ đọc, các ứng dụng trên máy tính và iOS, connect.dmsg.net, dịch vụ đám mây và các canister trên Internet Computer (ICP). “Chúng tôi” là nhóm vận hành dMsg dưới sự quản trị của ICPanda DAO. Khả năng sử dụng từng tính năng phụ thuộc vào phiên bản ứng dụng và cấu hình dịch vụ.

Chúng tôi không bán dữ liệu cá nhân cũng như không sử dụng nội dung của bạn cho mục đích quảng cáo. Trang web không chứa bất kỳ trình theo dõi quảng cáo hay phân tích nào.

## 1. Nội dung và thông tin công khai

Các mục trong Kho lưu trữ (Vault), tin nhắn riêng tư và tệp tin đều được mã hóa trước khi tải lên. Khóa nội dung luôn nằm trên các thiết bị được ủy quyền hoặc trong các gói phong bì khôi phục/thiết bị mà dịch vụ không thể mở. Người nhận có thể đọc và lưu giữ các bản sao được chia sẻ. Hồ sơ công khai, ảnh đại diện, định danh công khai (handle), tài liệu danh bạ và những thông tin bạn chủ đích đăng tải đều mang tính công khai. Báo cáo vi phạm là một thông tin tiết lộ riêng biệt: kiểm duyệt viên chỉ nhận được lý do, ghi chú, mã định danh tài khoản/tài nguyên và phần văn bản tin nhắn mà bạn chủ động chọn đính kèm. Việc gửi báo cáo không cung cấp khóa tài khoản của bạn cho kiểm duyệt viên.

## 2. Siêu dữ liệu dịch vụ

Dịch vụ xử lý mã định danh tài khoản và đăng nhập, khóa công khai của thiết bị, tư cách thành viên kênh, quy tắc liên hệ và chặn, kích thước đối tượng, thời điểm gửi yêu cầu, địa chỉ IP và thông tin yêu cầu vận hành. Trạng thái tài khoản đã chứng thực, biên nhận chữ ký và quyền sử dụng (entitlement) cho phép các ứng dụng khách (client) và dịch vụ xác minh thẩm quyền. Các giao dịch trên sổ cái (ledger) cùng các bản ghi công khai hoặc đã chứng thực trên chuỗi (on-chain) nhìn chung không thể xóa bỏ.

Nếu bạn bật thông báo, dịch vụ thông báo cũng xử lý những thông tin cần thiết để lọc cảnh báo trước khi gửi: mã định danh tài nguyên dạng ẩn (opaque), những kênh bạn đã tắt tiếng hoặc ẩn, khung giờ yên lặng và múi giờ, xác nhận đã xem (thời điểm bạn thực sự xem lời nhắc và đã đọc đến tin nhắn nào), và những tài khoản được đăng ký trên cùng một bản cài đặt. Để nhắc hạn cho một yêu cầu khôi phục tài khoản đang chờ, thiết bị của bạn tải lên bản ghi khôi phục, bao gồm danh tính đăng nhập mới và khóa công khai của thiết bị; dịch vụ xác minh và chỉ giữ lời nhắc cùng thời điểm thực hiện. Dịch vụ không nhận tên kênh, nội dung tin nhắn hay khóa để giải mã chúng. Thu hồi đăng ký, xóa tài khoản khỏi thiết bị hoặc xóa tài khoản dịch vụ sẽ xóa các tùy chọn, xác nhận đã xem và liên kết cài đặt này; bản ghi gửi và thử lại chỉ được giữ đến khi hết hạn.

## 3. Dịch vụ của Apple và thanh toán

Trên iOS, tính năng thông báo tùy chọn sử dụng dịch vụ Thông báo đẩy của Apple (APNs). Apple và dịch vụ chuyển phát của chúng tôi xử lý mã định danh cài đặt, token thiết bị và siêu dữ liệu chuyển phát. Thông báo chỉ chứa lời nhắc chung và lộ trình dạng ẩn, tuyệt đối không chứa nội dung tin nhắn, mật khẩu hay tên tệp. Bạn có thể tắt thông báo trong dMsg hoặc trong cài đặt hệ thống; những thông báo đã gửi đi không phải lúc nào cũng thu hồi được. Nếu nhiều tài khoản cùng nhận thông báo trên một bản cài đặt, token thiết bị dùng chung cho phép Apple và dịch vụ chuyển phát của chúng tôi liên kết các tài khoản đó; dMsg sẽ hỏi ý kiến bạn trước khi bật thông báo cho một tài khoản khác.

App Attest xử lý mã định danh khóa do ứng dụng tạo, bằng chứng chứng thực/xác nhận và các thử thách (challenge) để xác minh tính hợp lệ của bản cài đặt ứng dụng. Face ID, Touch ID và mật mã thiết bị do hệ điều hành trực tiếp xử lý; dMsg không nhận hình ảnh sinh trắc học hay mật mã thiết bị của bạn.

Các giao dịch mua qua StoreKit do Apple xử lý. Dịch vụ xác minh và canister thương mại của chúng tôi xử lý mã định danh giao dịch/sản phẩm, token liên kết tài khoản, ngày mua và ngày hết hạn cũng như trạng thái hoàn tiền/thu hồi quyền. Chúng không nhận thông tin thẻ thanh toán của bạn. Các ứng dụng khách dMsg khác có thể hỗ trợ thanh toán qua ví ICP và gói thành viên PANDA, kèm theo bản ghi sổ cái và quyền sử dụng công khai. Việc xóa tài khoản dMsg không tự động hủy gói đăng ký Apple; vui lòng quản lý gói đăng ký trong Tài khoản Apple của bạn.

Chính sách và đề xuất của ví đa chữ ký (multisig) được xử lý bởi các canister ví; phạm vi hiển thị của chúng tuân theo cài đặt của từng ví. Các nhà cung cấp RPC blockchain nhận địa chỉ ví, dữ liệu giao dịch và thông tin IP cần thiết để truy vấn số dư, mô phỏng phí và phát tán (broadcast) giao dịch. Các điểm cuối mặc định bao gồm mempool.space, PublicNode, mainnet.base.org và api.mainnet-beta.solana.com; các điểm cuối thay thế do bạn tự cấu hình tuân theo chính sách quyền riêng tư riêng của từng bên.

## 4. Dữ liệu cục bộ và tích hợp hệ thống

Các ứng dụng gốc (native) lưu giữ không gian làm việc mã hóa cục bộ, các khóa được hệ điều hành bảo vệ (nếu thiết bị hỗ trợ), bản nháp, thao tác đang chờ xử lý và tùy chọn cài đặt. Khi mở khóa, nội dung được chọn sẽ khả dụng trong bộ nhớ ứng dụng. Các tệp đã xuất và văn bản đã sao chép có thể được lưu lại bởi các ứng dụng đích.

Tiện ích mở rộng Chia sẻ (Share) trên iOS chỉ chuẩn bị tạm thời các mục bạn chọn để nhập vào tài khoản sau đó. Tính năng Tự động điền mật khẩu (AutoFill) và tích hợp với ứng dụng Tệp (Files) chỉ chứa các bản chụp nhanh (snapshot) được bạn chọn rõ ràng và mã hóa riêng biệt. Tự động điền yêu cầu xác thực hệ thống độc lập; các bản chụp nhanh sẽ hết hạn sau 24 giờ. Phiên truy cập ứng dụng Tệp kéo dài tối đa năm phút và kết thúc ngay khi dMsg khóa, nhưng ứng dụng Tệp và các bên nhận khác có thể vẫn giữ lại những bản sao đã tải xuống trước đó. Việc chỉnh sửa mục gốc không tự động cập nhật bản chụp nhanh đã xuất.

Các tác vụ truyền dữ liệu nền do bạn chủ động khởi tạo chỉ lưu trữ bản mã có giới hạn và quyền truyền dữ liệu trong một thư mục thiết bị riêng biệt, có thể truy cập sau lần mở khóa thiết bị đầu tiên. Chúng không lưu giữ khóa ký tài khoản hay khóa nội dung. Tài khoản gốc phải được mở khóa để xác minh và hoàn tất quá trình truyền. Hệ điều hành có thể trì hoãn hoặc dừng việc chuyển phát.

Trang web ghi nhớ lựa chọn ngôn ngữ của bạn ở bộ nhớ cục bộ. Ứng dụng web bản cũ sử dụng bộ nhớ trình duyệt cho các phiên làm việc và dữ liệu đệm (cache); chế độ xem ví của ứng dụng đó có thể truy vấn giá từ ICPSwap. Việc gỡ cài đặt ứng dụng hoặc xóa dữ liệu ứng dụng có thể làm mất vĩnh viễn những nội dung chưa từng được đồng bộ hóa hoặc xuất ra.

## 5. Nhà cung cấp dịch vụ và thời gian lưu trữ

Cloudflare xử lý bản mã đám mây và siêu dữ liệu yêu cầu. Các nhà cung cấp nút (node) và cổng kết nối (gateway) ICP vận hành các canister và trang web. Apple phân phối ứng dụng iOS và cung cấp các dịch vụ tùy chọn nêu trên. Internet Identity, các nhà cung cấp ví và các ứng dụng được kết nối đều có chính sách quyền riêng tư riêng. Chúng tôi có thể tiết lộ thông tin mà mình nắm giữ khi pháp luật yêu cầu, bao gồm cả các báo cáo vi phạm được gửi rõ ràng; chúng tôi không thể tiết lộ bản rõ (plaintext) mà chúng tôi không hề nắm giữ.

Bản mã trên đám mây tuân theo quy tắc lưu trữ và xóa của tài khoản. Yêu cầu xóa tài khoản dịch vụ là một quy trình dọn dẹp có theo dõi tiến độ: chặn quyền truy cập dịch vụ mới, xóa nội dung đám mây thuộc sở hữu của tài khoản cùng các bản sao ảnh đại diện công khai, và chỉ báo cáo hoàn tất sau khi đã dọn dẹp xong. Thao tác này không xóa các bản sao đã chia sẻ cho người khác, bản sao cục bộ trên các thiết bị khác, lịch sử sổ cái công khai hoặc các bản ghi tài chính/chống phát lại (anti-replay) cần thiết. Các giao dịch chưa tất toán hoặc các kênh đang hoạt động thuộc sở hữu của tài khoản có thể cần được giải quyết trước. Bản ghi báo cáo vi phạm được lưu giữ tối đa 90 ngày để xem xét; token chuyển phát sẽ hết hạn nếu không được gia hạn. Nhật ký vận hành chỉ được lưu giữ trong phạm vi cần thiết để đảm bảo an toàn, ngăn chặn lạm dụng và khắc phục sự cố.

## 6. Lựa chọn, quyền lợi và liên hệ

Bạn có thể xuất hoặc xóa nội dung, thu hồi quyền của thiết bị hoặc trình duyệt, chặn người gửi, báo cáo hành vi lạm dụng, thay đổi quy tắc liên hệ, yêu cầu xóa tài khoản dịch vụ và xóa riêng dữ liệu tài khoản trên thiết bị này. Tùy thuộc vào nơi cư trú, bạn có thể có quyền truy cập, đính chính, xóa hoặc phản đối việc xử lý thông tin cá nhân. Vui lòng liên hệ [Bộ phận Hỗ trợ](https://dmsg.net/_/support) nếu có yêu cầu hoặc muốn báo cáo riêng lỗ hổng bảo mật. Chúng tôi sẽ giải thích rõ khi cơ chế mã hóa, hồ sơ giao dịch phải lưu giữ hoặc các bản ghi công khai bất biến làm giới hạn phạm vi đáp ứng yêu cầu.

Không có hệ thống nào an toàn tuyệt đối. Các bản cập nhật ứng dụng và nâng cấp canister dưới sự quản trị của DAO vẫn là những điểm cần sự tin cậy. dMsg không dành cho trẻ em dưới 13 tuổi hoặc dưới độ tuổi tối thiểu theo quy định để sử dụng dịch vụ trực tuyến mà không có sự đồng ý của cha mẹ. Khi có thay đổi quan trọng về chính sách, chúng tôi sẽ cập nhật ngày ở đầu trang và thông báo trong ứng dụng hoặc trên trang web.
