## v0.10.2 — Chèn SFX trong vùng I/O Timeline

- Bỏ yêu cầu nguồn phải dài bằng toàn bộ vùng I/O.
- SFX ngắn giữ nguyên; SFX dài cắt tại Out; đoạn In/Out trên waveform được tôn trọng.
- Tìm track trống và xác minh kết quả theo độ dài thực tế vừa chèn.
- Danh sách Liên quan nhất tự ưu tiên SFX vừa vùng sau khi đọc I/O.

## v0.10.1 — Auto Bin, kéo thả SFX và track thông minh

- **Auto Bin:** chọn phạm vi → bật/tắt nhóm, đổi tên Bin hoặc chọn Bin có sẵn → Quét → xem trước → Sắp xếp. Timeline/Compound mặc định tắt. Khôi phục tên mặc định chỉ đổi cấu hình trong plugin, không đổi Bin đã có trong Resolve.
- **Kéo thả:** giữ tay nắm ⠿ cạnh tên SFX và kéo vào Resolve. Nếu file chưa sẵn sàng, thả chuột rồi kéo lại. Kéo file gốc đầy đủ; In/Out và track thông minh không áp dụng cho thao tác kéo. Muốn xuất đoạn riêng, dùng Export WAV; muốn chèn đoạn, dùng nút Chèn.
- **Track tự động:** mở tùy chọn cạnh danh sách audio track. Ưu tiên track chọn, rồi A1 xuống dưới; kiểm tra toàn bộ vùng chèn và chừa 2 frame cuối. Bỏ qua track khóa/tắt. Khi hết chỗ có thể tạo track mới trên cùng (A1, các track cũ dịch xuống) hoặc dưới cùng. Không tự tạo nếu bỏ tích Tạo track khi cần. Tắt tự tìm để dùng cách chèn thủ công trước đây.
- Các tùy chọn audio lưu lại giữa các lần mở. Chèn tại playhead và In/Out Timeline đều hỗ trợ track thông minh.
- Đóng Resolve, giải nén gói và chạy install_windows.bat. Installer giữ dữ liệu thư viện hiện tại.

## v0.10.0 — sửa Ctrl+Z và tùy chỉnh Auto Bin

Sửa kẹt Auto Bin sau Ctrl+Z: đối chiếu ID/vị trí hiện tại, nhận clip đã về Bin gốc dù Bin đích không còn. Liệt kê clip ở nơi khác hoặc không tìm thấy; có Kết thúc lần cũ sau xác nhận để lưu trữ nhật ký, giữ hiện trạng và quét lại. Tùy chỉnh nhóm và tên Bin đích, chọn Bin con có sẵn. Timeline/Compound mặc định tắt; Timeline theo ID project, Compound chỉ khi Resolve trả đúng loại Compound Clip. Giữ phiên bản 0.10.0.

Cách xử lý tình trạng cũ: mở đúng project → Auto Bin → Quét / Làm mới xem trước. Clip đã về nguồn được tự ghi nhận. Nếu còn mục không tìm thấy/đã chuyển chỗ, xem Đối chiếu lần Auto Bin trước. Chọn Hoàn tác để thử phục hồi mục còn ở đích, hoặc Kết thúc lần cũ → Xác nhận giữ hiện trạng để lưu nhật ký và bỏ khả năng hoàn tác lần đó trong plugin. Thao tác kết thúc không đổi Media Pool. Sau đó Quét lại.

Tùy chỉnh: bật/tắt nhóm → nhập tên hoặc chọn Bin có sẵn trong phạm vi → Quét lại → kiểm tra tên đích → Sắp xếp. Cấu hình tên/nhóm được nhớ trên máy, nhưng ID Bin có sẵn không lưu qua phiên. Mọi thay đổi cấu hình đều hủy bản xem trước cũ. Không dùng tên clip để đoán Compound. Chỉ xử lý media trực tiếp; không sắp xếp lại nội dung Bin con.

## v0.10.0 — cập nhật icon Auto Bin

Bin mới: 01_🎬 VIDEO, 02_🎵 AUDIO, 03_🖼 IMAGE, 04_📦 OTHER. Bin tên cũ được dùng lại, không tạo trùng và không tự đổi tên. Có thể đổi tên Bin cũ trong Resolve đúng theo các mẫu trên. Nếu đồng thời tồn tại tên cũ và tên mới của cùng nhóm, Auto Bin sẽ báo để người dùng xử lý trước. Mã phiên bản vẫn là 0.10.0.

# v0.10.0 — Quick Auto Bin

Quick Auto Bin: quét media trực tiếp trong Master hoặc Bin hiện tại, xem trước phân loại, tạo/dùng lại Bin thật 01_VIDEO, 02_AUDIO, 03_IMAGE, 04_OTHER trong Resolve. Giữ nguyên Bin con; bỏ qua timeline/clip đặc biệt hoặc loại chưa xác định an toàn. Kiểm tra thay đổi trước khi chạy, ghi nhật ký và hỗ trợ hoàn tác lần gần nhất. Nếu lỗi sẽ thử phục hồi media đã chuyển; giữ Bin trống. Bao gồm bản sửa installer không phụ thuộc PATH.

Cách dùng: mở project Resolve → Auto Bin → chọn phạm vi (mặc định Master) → Quét Media Pool → xem số lượng/danh sách → Sắp xếp. Không quét nội dung Bin con. Bin đích được tạo trong phạm vi đã chọn; nguồn media trên ổ đĩa không bị đổi tên/di chuyển.

Hoàn tác lần gần nhất: mở đúng project, vào Auto Bin và xác nhận Hoàn tác. Nhật ký ở máy hiện tại, không đi theo gói backup thư viện SFX. Lần Auto Bin thành công mới thay thế lịch sử hoàn tác trước. Nếu có lỗi phục hồi, phải giải quyết trước lần sắp xếp tiếp theo. Không hứa hỗ trợ Ctrl+Z của Resolve; không tự xóa Bin trống. Trước lần dùng đầu tiên, nên thử trên bản sao project để xác nhận hành vi của phiên bản Resolve đang dùng.

# v0.9.10

Tối ưu thumbnail cho ổ đọc chậm: chỉ tải clip đang hiện trong khung danh sách, chờ khoảng 180 ms sau khi ngừng cuộn, tối đa một yêu cầu mỗi lần. Hủy thumbnail đang xử lý nếu đã cuộn khỏi màn hình; bỏ việc cũ khi đổi tab/bộ lọc. Giữ thumbnail đã tải và trạng thái tab; List/Details không tải thumbnail.

# v0.9.9

Giữ trạng thái tab trong phiên làm việc: danh sách và thumbnail đã tải, trang/vị trí cuộn, file đang chọn, vị trí preview video, In/Out và tùy chọn chèn. Chuyển tab dừng phát; quay lại không tự phát hoặc tải lại preview đã mở. Ảnh/video giữ trạng thái riêng; các file chọn nhiều cũng được nhớ theo tab. Tự cập nhật danh sách nếu dữ liệu thư viện thay đổi.

Trạng thái preview/In-Out chỉ giữ trong lần mở plugin hiện tại. Khi đóng plugin, phần này không được lưu sang phiên mới. Thumbnail tải dở có thể được tải lại khi quay về tab; thumbnail đã xong được giữ. Danh sách chỉ dựng lại khi dữ liệu/bộ lọc thay đổi.

# v0.9.8

Sắp xếp Giới thiệu & Hướng dẫn: nhà phát triển, thiết lập ban đầu, hướng dẫn từng chức năng. Tách giải mã thumbnail sang tiến trình riêng, hủy tác vụ cũ và giới hạn 6 giây mỗi file. Auto preview âm thanh luôn bật khi chọn SFX; vẫn giữ âm lượng/tắt tiếng riêng.

Thumbnail dùng bộ giải mã media của Electron. File có codec không hỗ trợ hoặc quá thời gian sẽ hiện hình thay thế; vẫn có thể import/chèn qua Resolve.

# 0.9.7 — Tách cập nhật và giữ nút chèn luôn hiển thị

Cập nhật mở cửa sổ riêng gồm kiểm tra phiên bản, tự kiểm tra, changelog và lịch sử offline. Giới thiệu & Hướng dẫn giữ nội dung giới thiệu, liên hệ và hướng dẫn; bỏ phần cập nhật/lịch sử khỏi cửa sổ này. Thông báo Có gì mới sau nâng cấp vẫn hoạt động.

Tab ảnh/video có vùng preview/tùy chọn cuộn riêng. Thanh Track, Import, Chèn tại Playhead và trạng thái thao tác cố định dưới khung, không đè lên nội dung. Nút Tải lại track/Mở thư mục nằm trong vùng cuộn. Thông báo dài có thể cuộn trong ô trạng thái để thanh chèn không chiếm hết màn hình.

Đổi Thư viện thành Cài đặt; + Thư viện thành + Thư mục. Tên trường nguồn và dữ liệu không đổi. Sửa thời điểm cập nhật thông tin đoạn video sau khi metadata tải xong để tránh báo In/Out chưa hợp lệ khi vừa chọn clip.

Cài: đóng Resolve, giải nén rồi chạy install_windows.bat bằng Administrator.

---

# 0.9.6 — Quản lý cache và thông tin chẩn đoán

Thư viện → Cache & chẩn đoán → Kiểm tra dung lượng để xem số file và dung lượng waveform/thumbnail trên ổ đĩa. Xóa từng loại với bước xác nhận. Chỉ xóa file cache đúng tên hash và phần mở rộng trong hai thư mục cache; bỏ qua file lạ và thư mục con, từ chối thư mục cache dạng liên kết. Không thay tags, Favorites, index, bản backup, identity cache hoặc file tài nguyên gốc.

Preview lần sau tạo lại cache nếu cần. Cache đang được sử dụng có thể được ghi lại trong lúc/sau khi xóa. Xóa thumbnail đồng thời bỏ cache chuỗi trong giao diện và hủy tác vụ thumbnail chờ; ảnh đã hiển thị có thể vẫn còn trên màn hình. Xóa waveform không dừng audio đang nghe. Kiểm tra dung lượng không đo toàn bộ RAM hoặc mọi dữ liệu plugin.

Xuất thông tin chẩn đoán tạo JSON mới chứa phiên bản plugin/Electron/Chrome/Node, nền tảng và kiến trúc hệ điều hành, trạng thái kết nối bridge, số thư viện/file theo loại, số file có tag/yêu thích và dung lượng cache. Không xuất tên file, đường dẫn thư viện, nội dung tag, tài nguyên hoặc nhật ký lỗi chi tiết. Người dùng chọn nơi lưu rồi gửi file nếu cần hỗ trợ; plugin không tự gửi dữ liệu. Chọn tên mới, không ghi đè file đã có. Nếu cache không đọc được, báo cáo vẫn xuất với trạng thái cache không khả dụng.

Cài: đóng Resolve, giải nén và chạy install_windows.bat bằng Administrator.

---

# 0.9.5 — Tối ưu thư viện lớn và preview

Tối ưu tra cứu Favorites, lưu kết quả thống kê tags, chuẩn hóa chuỗi tìm kiếm và sắp xếp tên. Đọc giá trị bộ lọc một lần mỗi lượt thay vì truy cập giao diện cho từng file. Chọn SFX chỉ cập nhật dòng đang chọn và preview, không dựng lại toàn bộ danh sách.

Thư viện trên 10.000 tài nguyên chờ 120 ms sau lần gõ cuối rồi tìm kiếm để tránh tính lại mỗi ký tự. Chọn ảnh/video chờ 70 ms để bỏ lựa chọn đã cũ trước khi mở nguồn. Kết quả preview cũ không được ghi đè file đang chọn.

Thumbnail: giữ tối đa 2 yêu cầu đang chờ trên toàn giao diện, bỏ các tác vụ chờ của danh sách cũ, không giữ kết quả giải mã native đã hết hiệu lực. Dữ liệu chuỗi thumbnail cache tối đa 16 MiB và 80 mục; đây không phải giới hạn tổng RAM của plugin. Thumbnail/codec native đã chạy không thể ngắt cưỡng bức, nhưng kết quả cũ không được dùng. List/Details vẫn không tạo thumbnail. Waveform chỉ giải mã một file mỗi lần và bỏ các lựa chọn chờ đã cũ.

Số đo trên dữ liệu giả lập 50.000 tài nguyên, trung vị 3 lượt Edge headless: dựng danh sách 1171 → 93 ms; tìm kiếm 1116 → 75 ms; chuyển Video 1521 → 251 ms. Không bao gồm đọc ổ đĩa, scan, độ trễ chờ gõ và giải mã video; chưa đo trong Resolve thật. Xem PERFORMANCE.md để biết phương pháp và giới hạn.

Không đổi cách lưu tags, Favorites, backup, Export WAV hoặc logic chèn Resolve. Chưa thêm proxy video hay công cụ dọn cache.

Cài: đóng Resolve, giải nén, chạy install_windows.bat bằng Administrator.

---

# 0.9.4 — Preview video và nghe thử đoạn In/Out

Video có Phát/Dừng, Phát đoạn In/Out, Lặp đoạn, Về In và Về Out. Hiển thị In, Out và độ dài đoạn theo giây. Đoạn không hợp lệ sẽ báo và khóa nút phát đoạn. Về In/Out dừng preview trước khi tua. Chuyển file/tab dừng video và bỏ chế độ phát đoạn; không tự phát khi mở lại plugin.

Khi focus danh sách/preview video: Space phát/dừng; ←/→ tua 1 giây; Shift + ←/→ tua 5 giây; I/O đánh dấu. Không bắt phím khi gõ trong ô tìm kiếm, tags, ô số, menu chọn hoặc điều khiển nút thông thường. Tua giới hạn trong chiều dài video. Phát thường dùng đoạn đã chọn nếu Chèn đoạn In/Out đang bật; nút Phát đoạn In/Out luôn phát đoạn hiển thị. Tua bằng phím thoát chế độ phát đoạn đang chạy.

Âm lượng nghe thử và tắt tiếng video được lưu riêng trên máy, độc lập preview SFX và không đưa vào thông số chèn Resolve. Điều khiển âm lượng có sẵn trong video cũng đồng bộ vào thiết lập này. Không đổi gain của file hoặc clip được chèn. In/Out và tùy chọn lặp không được lưu qua lần mở lại; định dạng/codec preview phụ thuộc bộ giải mã Electron.

Lặp/dừng preview theo thời gian bộ phát, không đảm bảo chính xác từng frame; logic In/Out gửi sang Resolve giữ nguyên. Chưa có stepping từng frame hoặc playback proxy.

Cài: đóng Resolve, giải nén, chạy install_windows.bat bằng Administrator.

---

# 0.9.3 — Nối lại thư viện khi đổi ổ/thư mục

Vào Thư viện → Nối lại cạnh thư viện cũ → Chọn thư mục mới → Đối chiếu file → kiểm tra kết quả → Áp dụng. Không cần xuất backup trước đó nếu index hoặc tags/yêu thích của thư viện vẫn còn. Không di chuyển hay sửa file nguồn.

Dùng index/metadata hiện tại và mã nhận diện nội dung đã lưu từ lần xuất backup trước, nếu có. Khi không có mã nhận diện, plugin không giả định file trùng tên là cùng nội dung. Các file này nằm ở danh sách cần xác nhận. Nút Chọn các mục khớp vị trí / kích thước chỉ chọn những mục có một ứng viên duy nhất, đúng đường dẫn tương đối và kích thước đã biết; người dùng vẫn bấm Áp dụng sau khi kiểm tra. File trùng nhiều nơi hoặc không khớp cần chọn riêng/để nối lại sau. Chỉ tìm ứng viên trong thư mục đích đã chọn.

Tags/yêu thích được gộp, không xóa metadata cũ. Waveform chỉ chuyển khi mã nội dung trùng khớp. Có thể chọn gộp Smart Collections, Recent/lượt sử dụng như luồng khôi phục hiện có. Tạo bản sao trước áp dụng và dùng Hoàn tác lần nhập để quay lại nếu cần. Các chỉnh sửa metadata sau lần nhập cũng sẽ bị hoàn tác nếu dùng nút này.

Thư viện cũ được giữ để đối chiếu, chưa tự xóa hoặc thay thế. Sau khi xác nhận mọi file cần thiết đã nối đúng, có thể bấm Xóa cạnh thư viện cũ để bỏ khỏi danh sách (không xóa file ổ cứng). Mục chưa tìm thấy nằm trong Nối lại mục còn thiếu. Nếu index đã mất, chỉ còn tags/yêu thích, plugin vẫn cố tìm theo tên nhưng cần chọn thủ công. Nếu mất cả metadata thì cần mở bản sao lưu cũ.

Cài: đóng Resolve, giải nén và chạy install_windows.bat bằng Administrator. Bridge và các chức năng v0.9.2 được giữ nguyên.

---

# 0.9.2 — Tags cho cả âm thanh, ảnh và video

Ảnh/video có sidebar Tags: tìm theo tên, đếm số file trong toàn bộ tab, chọn nhiều tag theo Có tất cả / Có ít nhất một, kết hợp bộ lọc hiện tại. Lựa chọn tags được nhớ riêng từng tab. Số đếm theo tab, không phải số kết quả sau khi lọc.

Chọn ảnh/video → gõ tag ở ô Gõ tag rồi Enter → Enter hoặc + Tag. Gợi ý lấy từ cả ba loại tài nguyên. Bấm × trên nhãn để gỡ khỏi file đang chọn. Chọn nhiều vẫn cho thêm/gỡ tags hàng loạt. Sửa toàn bộ tags bằng ô cũ vẫn hoạt động.

Quản lý tags trong sidebar cho phép đổi tên hoặc gộp vào tag đã có. Cửa sổ cho biết số file bị tác động trước khi bấm Đổi tên / Gộp tag. Áp dụng trên toàn bộ metadata đang dùng, gồm cả đường dẫn chưa có trong index; không đổi file nguồn, không sửa backup/pending restore cũ. Tránh trùng hoa/thường và Unicode, cập nhật Smart Collections cùng các bộ lọc liên quan. Tạo bản sao settings trước thay đổi tại thư mục dữ liệu; đường dẫn hiện sau thao tác. Đây là bản sao kỹ thuật settings, không phải file portable để nhập trực tiếp.

Sửa backup để giữ danh sách nhiều tags và điều kiện tất cả/ít nhất một của Smart Collections. Giao diện List/Details/Thumbnail, Export WAV và bridge giữ nguyên.

Cài: đóng Resolve, giải nén, chạy install_windows.bat bằng Administrator. Không cần chuyển đổi database.

---

# 0.9.1 — Chế độ xem tài nguyên và nhớ trạng thái từng tab

Ảnh và Video có List / Details / Thumbnail. Video mặc định List, ảnh mặc định Thumbnail. List và Details không tạo thumbnail; chọn file vẫn mở preview. Thumbnail tải ảnh theo vùng nhìn thấy với hai tác vụ đồng thời và chỉnh được kích thước 100–240 px. Mỗi trang tối đa 48 file.

Sắp xếp theo tên (có thứ tự số tự nhiên), định dạng, thời lượng, ngày thêm hoặc ngày sửa; đổi chiều tăng/giảm. Details có thể bấm tên cột để sắp xếp. Ngày/thời lượng chưa biết để cuối; không quét toàn bộ video để lấy thời lượng khi đổi kiểu xem. Thời lượng biết sau preview sẽ cập nhật dòng tương ứng; lần sắp xếp tiếp theo sử dụng số mới. Ảnh không có thời lượng nguồn, hiển thị dấu —. Ngày thêm là ngày được ghi nhận trong thư viện, không phải ngày tạo file. Khi cửa sổ nhỏ, cuộn ngang trong Details để xem các cột cuối.

Nhớ riêng tìm kiếm, thư mục, yêu thích/gần đây, lọc ngày/tag, kiểu xem, cỡ thumbnail, sắp xếp, trang, file đang chọn và vị trí cuộn cho ảnh/video. Tab âm thanh giữ bộ lọc, file đang chọn và vị trí cuộn. Khi mở lại plugin khôi phục tab cuối; không tự phát preview. Nếu file không còn trong dữ liệu/kết quả hiện tại thì bỏ chọn; trang và vị trí cuộn được giới hạn theo kết quả còn lại. Không khôi phục đoạn In/Out và vị trí phát video ở bản này. Trạng thái duyệt lưu cục bộ trên máy, chưa nằm trong bản sao lưu portable.

Cài: đóng Resolve, giải nén rồi chạy install_windows.bat bằng Administrator. Giữ database và bridge hiện tại.

---

# 0.9.0 — Changelog tổng hợp, Export WAV và bộ lọc thường trực

Cập nhật và Giới thiệu & Hướng dẫn hiện trực tiếp cạnh tên app. Hai hàng lọc luôn hiện: ngày thêm, định dạng, thư viện, sắp xếp, thời lượng, tag và Smart Collections. Tùy chọn chứa Auto preview, hiển thị tag và thao tác lưu/xóa bộ lọc.

Changelog: kiểm tra bản mới tổng hợp các release sau bản đang dùng đến bản đích, sắp xếp mới nhất trước. Nếu GitHub không trả đủ lịch sử, plugin báo rõ. Khi vừa nâng cấp, popup hiển thị các bản đã bỏ qua; bấm Đã xem để ngừng nhắc. Có thể xem lại lịch sử trong Giới thiệu & Hướng dẫn kể cả khi offline. Installer ghi nhận phiên bản cũ để hỗ trợ người nâng từ 0.6.1; nếu không nhận diện được sẽ thông báo và hiện toàn bộ lịch sử đi kèm. Bản cũ chưa có cơ chế tổng hợp sẽ chỉ thấy lịch sử đầy đủ sau khi cài 0.9.0 bằng installer mới.

Export: chọn SFX → đánh In/Out → Export WAV → chọn tên file mới. Nếu không chọn đoạn, xuất toàn bộ SFX. Xuất WAV PCM 24-bit, 48 kHz, mono hoặc stereo theo nguồn. Âm lượng/tắt tiếng preview không áp dụng lên file xuất. Không sửa file gốc, không ghi đè file đã có; không cần cài FFmpeg. Dùng In/Out của SFX, không phải dấu trên timeline. Nguồn tối đa 64 MiB và 20 phút; đầu ra tối đa 96 MiB; nguồn phải được bộ giải mã của plugin hỗ trợ. Có thể có sai số giải mã codec nén; đây không phải cắt không giải mã.

Cài: đóng Resolve, giải nén ZIP và chạy install_windows.bat bằng Administrator. Không cần quét lại thư viện; giữ dữ liệu hiện có.

Cho người phát hành: mỗi bản cần thêm ghi chú vào release-history.json, tăng phiên bản và đăng GitHub Release chính thức với gói HNH_SFX_Finder_vX.Y.Z.zip. Giữ các release cũ để người nâng nhảy cóc đọc được ghi chú. Lịch sử đi kèm là bản tóm tắt, GitHub dùng nội dung release thực tế khi kiểm tra online.

---

# 0.8.5 — Video: tự chọn track và giữ vị trí cuộn

Trong Video bật Kèm âm thanh & liên kết clip. Tự tìm track trống mặc định bật: ưu tiên V/A đang chọn, nếu bận hoặc khóa thì tìm track khác. Kiểm tra toàn bộ chiều dài clip/đoạn In/Out cộng 2 frame dự phòng. Nếu không đủ chỗ sẽ báo, không ghi đè. Tạo track khi cần mặc định tắt; bật để thêm track video/audio stereo khi hết chỗ. Track mới trống có thể còn lại khi chèn lỗi; thông báo sẽ liệt kê. Tắt tự tìm để chọn track thủ công như trước. Nguồn nhiều track hoặc external linked audio vẫn chưa hỗ trợ.

Preview không dựng lại cả danh sách; cập nhật chọn và duration tại chỗ. Các lần refresh metadata/Favorites/tags giữ vị trí cuộn và focus; đổi bộ lọc, trang hoặc tab chủ động về đầu. Không cần cuộn lại xuống clip vừa bấm.

Thông báo lỗi chèn A/V hiện track, vị trí, số frame thực tế/mong đợi và FPS để chẩn đoán. Chấp nhận sai số độ dài tối đa 2 frame, báo khi vượt 1 frame; vị trí bắt đầu vẫn phải chính xác. Sai lệch lớn sẽ thử gỡ chỉ clip vừa tạo. Chưa xác định nguyên nhân cụ thể của file người dùng báo lỗi; chưa khẳng định sửa mọi lỗi A/V.

Cài: đóng Resolve, giải nén, chạy install_windows.bat bằng Administrator. Bridge và dữ liệu giữ nguyên.

---
# 0.8.4 — Sidebar Tags cho SFX

Sidebar Âm thanh có Thư mục / Tags. Tab Tags tìm nhanh theo tên, hiện số SFX được gắn tag trên toàn bộ thư viện audio, không chỉ các kết quả đang lọc. Chọn nhiều tags; Có tất cả hoặc Có ít nhất một. Các nhãn đang lọc hiện trên danh sách, bấm x để bỏ từng nhãn. Các bộ lọc folder/ngày/search vẫn kết hợp; Bỏ lọc tags chỉ xóa điều kiện tags. Smart Collections lưu được lựa chọn tags mới.

Chọn SFX → + Tag → gõ hoặc chọn tag gợi ý → Enter/Thêm. Giữ nguyên tags cũ, tránh trùng hoa/thường. Mỗi lần thêm một tag. Tag hiện ngay trên waveform; bấm x cạnh tag để gỡ khỏi file đang chọn. Mục Sửa toàn bộ tags vẫn giữ cách nhập bằng dấu phẩy cũ. Chọn nhiều ở thanh tìm kiếm để thêm/gỡ tags hàng loạt như trước. Tags tiếp tục được lưu/backup bằng dữ liệu cũ, không cần chuyển đổi database.

Phiên bản này áp dụng sidebar tags và thêm nhanh cho tab Âm thanh. Ảnh/Video vẫn dùng giao diện tags hiện có. Chưa có ghim, đổi tên/gộp tags hoặc bộ sưu tập thủ công. Đóng Resolve, giải nén và chạy install_windows.bat bằng Administrator để cài.

---
# 0.8.3 — Giao diện gọn

Gom bộ lọc SFX vào bảng nổi, hiển thị tóm tắt điều kiện đang áp dụng. Chuyển gắn tags xuống Sửa tags cạnh tên file đang chọn. Header một hàng, Cập nhật/Hướng dẫn trong menu ⋯ có dấu báo bản mới. Dòng SFX khoảng 44 px; ngày thêm xem qua tooltip. Waveform mặc định 40 px, kéo mép trên preview để đổi chiều cao hoặc dùng phím mũi tên khi focus thanh kéo. Chọn nhiều vẫn dùng được trên cả ba tab.

Đo bằng Edge headless: 9 dòng đầy đủ ở 1030x780; 6 dòng ở 760x680 với I/O Timeline. Kích thước font/DPI máy người dùng có thể thay đổi kết quả. Chưa thay đổi xử lý video, bộ sưu tập thủ công hoặc export âm thanh.

Cài: đóng Resolve → giải nén → chạy install_windows.bat bằng Administrator.

---
# HNH SFX Finder 0.8.2 — Chèn nhạc theo In/Out Timeline

Cài: đóng Resolve, giải nén gói, chạy install_windows.bat bằng Administrator. Dữ liệu thư viện và bridge v2.0.0 giữ nguyên.

1. Đánh dấu I và O trên timeline Resolve.
2. Chọn file nhạc trong tab Âm thanh. Điểm In của nhạc là điểm bắt đầu nguồn (mặc định 0 giây).
3. Chọn track A đích và Chèn theo → In/Out Timeline.
4. Kiểm tra thông tin vùng rồi bấm Chèn vào I/O Timeline.

Plugin đọc vùng khi quay lại cửa sổ, có nút Đọc lại I/O và luôn đọc lại trước khi chèn. Playhead không ảnh hưởng. Out của file nhạc chỉ dùng preview trong chế độ này. Chuyển về Playhead để dùng cách cũ; mỗi lần mở plugin mặc định Playhead.

Nhạc không đủ dài sẽ báo lỗi, chưa lặp hoặc kéo giãn. Track cần mở khóa, vùng chèn trống và chừa 2 frame sau Out để tránh va chạm do làm tròn. Hỗ trợ một track audio nguồn; mapping nhiều track hoặc audio liên kết ngoài cần xử lý trong Resolve. Ưu tiên marks audio, fallback video nếu không có marks audio. Không đủ hai mốc thì báo lỗi.

Khôi phục I/O sau chèn nếu Resolve xóa dấu; nếu người dùng đổi dấu trong lúc chèn thì giữ dấu mới. Sai số tối đa 2 frame được báo; sai lớn hơn hoặc sai vị trí/track sẽ thử gỡ clip mới. Nếu lỗi native hoặc không gỡ được, kiểm tra timeline trước khi thử lại.

Đã kiểm chứng API trên Resolve Studio 20.2.3 ở 23.976 fps, timeline bắt đầu 0 và 01:00:00:00, nguồn audio 23.976 và 29.97 fps. Phiên bản Resolve thiếu API sẽ báo rõ; không cam kết tính năng mới trên mọi phiên bản Studio. Hướng dẫn đã tích hợp trong plugin.

---

# 0.8.1 — 2026-09-27

Đưa mục Cập nhật lên đầu Giới thiệu & Hướng dẫn; thêm nút Cập nhật trên thanh đầu plugin. Hiển thị patch note từ mô tả GitHub Release ngay trong mục cập nhật, kể cả sau khi đóng popup. Nếu release không có mô tả, hiện thông báo thay thế.

Khi đăng bản mới, viết patch note trong phần mô tả Release và đính kèm ZIP đúng tên HNH_SFX_Finder_v0.8.1.zip. Cài: đóng Resolve, giải nén và chạy install_windows.bat bằng Administrator.

---

# 0.8.0 — 2026-09-26

- Thêm tùy chọn chèn video kèm audio, chọn riêng track V/A, chung In/Out và playhead, liên kết hai clip.
- Kiểm tra mapping nguồn, track khóa và vùng trống; kiểm tra kết quả chèn và thử gỡ clip mới khi phát hiện lỗi.
- Chọn nhiều audio/ảnh/video để thêm hoặc xóa tags, thêm/bỏ Favorites. Giữ metadata riêng và kiểm tra lựa chọn cũ trước khi lưu.
- Bổ sung hướng dẫn trong plugin. Giữ bridge, dữ liệu và luồng SFX cũ.

---

# 0.7.4 — 2026-09-26

Thêm kiểm tra bản mới từ GitHub Releases chính thức của Phạm Nam, kiểm tra nền mỗi ngày, kiểm tra thủ công, mở trang tải, hoãn nhắc và bỏ qua phiên bản. Chỉ báo khi có gói ZIP hợp lệ. Không tự cài hoặc thay đổi dữ liệu thư viện.

# 0.7.3 — 2026-09-26

Tách duration SFX thành nhãn canh phải, bỏ thông tin trùng ở dòng đường dẫn; thêm nhãn duration ở góc thumbnail video và cập nhật sau khi đọc metadata.

# 0.7.2 — 2026-09-26

Theo dõi ngày đầu tiên ghi nhận đường dẫn file, bảo toàn qua scan và backup. Thêm lọc ngày nhanh trên cả ba tab và danh sách tags có số file; giữ trạng thái ngày chưa rõ cho dữ liệu cũ.

# 0.7.1 — 2026-09-26

Thêm chọn loại quét cho từng thư viện và sửa lại sau. Giữ mặc định thư viện cũ, metadata của loại bị bỏ chọn và tính tương thích backup. Lọc trước stat, xử lý folder lồng nhau/offline theo loại cho phép.

# 0.7.0 — 2026-09-26

Thêm ba tab Âm thanh/Hình ảnh/Video, thư viện chung, thumbnail tải theo vùng nhìn thấy và cache, preview ảnh/video, tags/Favorites/Recent, lọc folder/tìm kiếm, import Media Pool, chèn ảnh có thời lượng và phần hình video có In/Out vào video track tại playhead. Kiểm tra track khóa và lựa chọn cũ; backup nhận cả metadata ảnh/video. Giữ native bridge và hàm chèn SFX cũ.

# 0.6.1 — 2026-09-26

Thêm nút Giới thiệu & Hướng dẫn, nội dung sử dụng các tính năng ngay trong plugin và liên hệ nhà phát triển Phạm Nam. Bảng hướng dẫn hỗ trợ bàn phím và ngăn phím tắt chèn SFX khi đang đọc.

# 0.6.0 — 2026-09-26

Thêm xuất/nhập .hnhbackup, chuyển tags/Favorites/index/settings và waveform tùy chọn. Nối lại thư mục theo mã nội dung SHA-256; xác nhận các trường hợp mơ hồ; giữ mục chờ; gộp dữ liệu, sao lưu trước nhập, hoàn tác và khôi phục khi ghi bị gián đoạn. Giữ bridge v2.0.0 và các chức năng audio/Resolve hiện có.

# 0.5.1 — 2026-09-26

Đọc index đã lưu trước, quét cập nhật nền, kết nối Resolve độc lập và tự retry. Hiển thị đường dẫn dữ liệu và trạng thái quét. Không đổi cách lưu database hoặc xử lý audio/Insert.

# 0.5.0 — 2026-09-26

Thêm tùy chọn tag cạnh tên và duyệt SFX theo cây thư mục. Nhớ tùy chọn, folder và hỗ trợ lưu folder vào Smart Collections. Giữ nguyên xử lý âm thanh và Resolve bridge từ 0.4.0.
