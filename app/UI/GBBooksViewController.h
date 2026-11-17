#import <UIKit/UIKit.h>

typedef NS_ENUM(NSInteger, GBBooksKind) {
    /// Thư viện: sách của mọi nguồn đang bật.
    GBBooksLibrary,
    /// Sách của một nguồn (`source`).
    GBBooksSource,
    /// Đã tải về máy.
    GBBooksOffline,
    /// Một bộ sưu tập / Yêu thích / Muốn đọc / Mở gần đây (`collectionId`: số, @"fav", @"want", @"recent").
    GBBooksCollection,
};

/// Danh sách sách: ô tìm (không dấu), xếp Mới / A–Z / Tác giả + thanh chữ cái, lọc định dạng / tình trạng đọc, chạm để đọc, «⋯»
/// mở bảng thao tác. Thư viện / nguồn / đã tải lấy từng trang từ lõi (GBBookList); bộ sưu tập lấy cả danh sách.
@interface GBBooksViewController : UITableViewController <UISearchBarDelegate, UIActionSheetDelegate>
- (instancetype)initWithKind:(GBBooksKind)kind;
@property (nonatomic, readonly) GBBooksKind kind;
@property (nonatomic, copy) NSString *source;
@property (nonatomic, strong) id collectionId;
@property (nonatomic, strong) NSNumber *topic;
@property (nonatomic) BOOL forum;
/// Vừa đăng nhập: tải lại thư viện.
- (void)loggedIn;
@end
