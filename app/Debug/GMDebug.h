#import <Foundation/Foundation.h>

/// Chỉ có trong bản thử (DEBUG, như GramMusic): đọc lệnh từ /var/mobile/Documents/GramBook-cmd.txt (mỗi dòng một lệnh, đọc xong xoá
/// tệp) để thử app qua SSH — kèm `tmshot` / `cbscreen` chụp màn hình. Kết quả ghi vào app.log («[thử] …»). Lệnh:
///   call <lệnh> [json]       gọi lệnh lõi (docs/FFI.md), ghi kết quả (rút gọn)
///   tab <0–4>                chọn mục thanh dưới (Đang đọc, Thư viện, Nguồn, Bộ sưu tập, Cài đặt)
///   select <mục> <hàng>      chạm hàng của bảng đang hiện
///   back | dismiss           quay lại màn trước / đóng màn đang phủ
///   open <khoá>              mở sách (khoá thư viện, hoặc file:/đường/dẫn.epub)
///   openfile <đường dẫn>     mở sách trên máy
///   r <lệnh> [đối số]        lệnh cho màn đọc: next, prev, chapter <n>, toc, aa, chrome, close, turn <curl|slide|none>, js <mã>, info
///   snap [tệp]               chụp cửa sổ app (PNG)
///   perform <hàm>            gọi hàm không tham số của màn trên cùng
@interface GMDebug : NSObject
+ (void)start;
@end
