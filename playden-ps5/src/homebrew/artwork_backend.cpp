#include "homebrew/artwork_backend.hpp"

#include "steam/backend/http_client.h"
#include "platform/managed_directory.h"
#include "platform/storage.h"
#include <mbedtls/sha256.h>
#include <unistd.h>

#include <algorithm>
#include <climits>
#include <cstddef>
#include <cstdlib>
#include <cstdint>
#include <limits>
#include <memory>
#include <string>
#include <utility>

namespace {

struct DecodeBudget {
  std::size_t limit = 0;
  std::size_t outstanding = 0;
};

struct alignas(std::max_align_t) AllocationHeader {
  std::size_t size = 0;
  DecodeBudget* owner = nullptr;
};

thread_local DecodeBudget* g_decode_budget = nullptr;

void* ArtworkAllocate(std::size_t size) {
  DecodeBudget* budget = g_decode_budget;
  if (!budget || size > budget->limit -
                            std::min(budget->outstanding, budget->limit) ||
      size > std::numeric_limits<std::size_t>::max() - sizeof(AllocationHeader))
    return nullptr;
  auto* header = static_cast<AllocationHeader*>(
      std::malloc(sizeof(AllocationHeader) + size));
  if (!header) return nullptr;
  header->size = size;
  header->owner = budget;
  budget->outstanding += size;
  return header + 1;
}

void ArtworkFree(void* allocation) {
  if (!allocation) return;
  auto* header = static_cast<AllocationHeader*>(allocation) - 1;
  if (header->owner)
    header->owner->outstanding -=
        std::min(header->size, header->owner->outstanding);
  std::free(header);
}

void* ArtworkReallocate(void* allocation, std::size_t size) {
  if (!allocation) return ArtworkAllocate(size);
  if (size == 0) {
    ArtworkFree(allocation);
    return nullptr;
  }
  auto* old_header = static_cast<AllocationHeader*>(allocation) - 1;
  DecodeBudget* budget = old_header->owner;
  if (!budget ||
      (size > old_header->size &&
       size - old_header->size >
           budget->limit - std::min(budget->outstanding, budget->limit)) ||
      size > std::numeric_limits<std::size_t>::max() - sizeof(AllocationHeader))
    return nullptr;
  auto* header = static_cast<AllocationHeader*>(
      std::realloc(old_header, sizeof(AllocationHeader) + size));
  if (!header) return nullptr;
  if (size >= header->size) budget->outstanding += size - header->size;
  else budget->outstanding -= header->size - size;
  header->size = size;
  header->owner = budget;
  return header + 1;
}

class DecodeBudgetScope {
 public:
  explicit DecodeBudgetScope(std::size_t limit)
      : prior_(g_decode_budget), budget_{limit, 0} {
    g_decode_budget = &budget_;
  }
  ~DecodeBudgetScope() { g_decode_budget = prior_; }

 private:
  DecodeBudget* prior_;
  DecodeBudget budget_;
};

}  // namespace

#define STB_IMAGE_STATIC
#define STBI_ONLY_JPEG
#define STBI_ONLY_PNG
#define STBI_NO_STDIO
#define STBI_NO_HDR
#define STBI_NO_LINEAR
#define STBI_MALLOC(size) ArtworkAllocate(size)
#define STBI_REALLOC(pointer, size) ArtworkReallocate(pointer, size)
#define STBI_FREE(pointer) ArtworkFree(pointer)
#define STB_IMAGE_IMPLEMENTATION
#if defined(__clang__) || defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-function"
#endif
#include "stb_image.h"
#if defined(__clang__) || defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

namespace playden::homebrew {
namespace {

constexpr std::uint32_t kArtworkTimeoutMs = 10'000;

bool CheckedImageBytes(int width, int height, std::size_t& bytes) {
  if (width <= 0 || height <= 0 ||
      width > static_cast<int>(kMaxArtworkDimension) ||
      height > static_cast<int>(kMaxArtworkDimension)) return false;
  const std::uint64_t value = static_cast<std::uint64_t>(width) *
                              static_cast<std::uint64_t>(height) * 4u;
  if (value > kMaxArtworkImageBytes ||
      value > std::numeric_limits<std::size_t>::max()) return false;
  bytes = static_cast<std::size_t>(value);
  return true;
}

// The game's own CDN art: its cover, its library hero and its logo, and nothing else.
bool CanonicalKey(const ArtworkKey& key) {
  if (key.account_generation == 0 || !key.game_id.valid() ||
      key.source.size() > kMaxArtworkSourceBytes) return false;
  if (key.game_id.store == Store::kXbox) {
    constexpr std::string_view origin = "https://store-images.s-microsoft.com/";
    return key.source.compare(0, origin.size(), origin) == 0 &&
           std::none_of(key.source.begin(), key.source.end(), [](unsigned char c) { return c < 32 || c == 127; });
  }
  const std::string base = "https://cdn.cloudflare.steamstatic.com/steam/apps/" +
                           ToKey(key.game_id) + "/";
  for (const char* name : {"library_600x900.jpg", "library_hero.jpg", "logo.png"})
    if (key.source == base + name) return true;
  return false;
}

}  // namespace

std::optional<ArtworkImage> artwork_internal::DecodeArtworkWithAllocationBudget(
    const std::vector<std::uint8_t>& encoded, std::size_t allocation_limit) {
  if (encoded.empty() || encoded.size() > kMaxArtworkTransferBytes ||
      encoded.size() > static_cast<std::size_t>(INT_MAX) || allocation_limit == 0 ||
      allocation_limit > kMaxArtworkDecodeAllocationBytes) return std::nullopt;
  DecodeBudgetScope budget(allocation_limit);

  int width = 0;
  int height = 0;
  int components = 0;
  if (!stbi_info_from_memory(encoded.data(), static_cast<int>(encoded.size()),
                             &width, &height, &components)) return std::nullopt;
  std::size_t bytes = 0;
  if (!CheckedImageBytes(width, height, bytes)) return std::nullopt;

  int decoded_width = 0;
  int decoded_height = 0;
  int decoded_components = 0;
  stbi_uc* decoded = stbi_load_from_memory(
      encoded.data(), static_cast<int>(encoded.size()), &decoded_width,
      &decoded_height, &decoded_components, 4);
  if (!decoded) return std::nullopt;
  std::unique_ptr<stbi_uc, decltype(&stbi_image_free)> pixels(decoded,
                                                              stbi_image_free);
  std::size_t decoded_bytes = 0;
  if (decoded_width != width || decoded_height != height ||
      !CheckedImageBytes(decoded_width, decoded_height, decoded_bytes) ||
      decoded_bytes != bytes) return std::nullopt;

  ArtworkImage image;
  image.width = static_cast<std::uint32_t>(decoded_width);
  image.height = static_cast<std::uint32_t>(decoded_height);
  image.rgba.assign(pixels.get(), pixels.get() + decoded_bytes);
  return image;
}

std::optional<ArtworkImage> DecodeArtwork(
    const std::vector<std::uint8_t>& encoded) {
  return artwork_internal::DecodeArtworkWithAllocationBudget(
      encoded, kMaxArtworkDecodeAllocationBytes);
}

HttpArtworkBackend::HttpArtworkBackend(std::string ca_bundle, std::string cache_root, Fetch fetch)
    : ca_bundle_(std::move(ca_bundle)), cache_root_(std::move(cache_root)), fetch_(std::move(fetch)) {}

HttpArtworkBackend::~HttpArtworkBackend() = default;

std::optional<ArtworkImage> HttpArtworkBackend::Load(
    const ArtworkKey& key, const std::atomic<bool>& cancel) {
  if (!CanonicalKey(key) || cancel.load(std::memory_order_acquire))
    return std::nullopt;
  // Public CDN images are shared across account generations. The full source
  // URL is the version key, so changed artwork/resolutions cannot reuse old bytes.
  unsigned char digest[32]{};
  mbedtls_sha256(reinterpret_cast<const unsigned char*>(key.source.data()),key.source.size(),digest,0);
  std::string name; const char* hex = "0123456789abcdef";
  for (const auto byte : digest) { name += hex[byte >> 4]; name += hex[byte & 15]; }
  name += ".image";
  std::optional<platform::ManagedDirectory> directory;
  if (!cache_root_.empty()) {
    const std::filesystem::path path(cache_root_);
    auto parent = platform::ManagedDirectory::Open(path.parent_path(),platform::DirectoryPolicy::kApprovedTree);
    if (parent) directory = parent->OpenChild(path.filename().string(),platform::DirectoryPolicy::kPrivate,true);
  }
  const auto read_cached = [&]() -> std::optional<ArtworkImage> {
    if (!directory) return std::nullopt;
    const int fd = directory->OpenRegularFile(name,true,0600);
    if (fd < 0) return std::nullopt;
    struct stat info{}; std::vector<std::uint8_t> bytes;
    if (fstat(fd,&info) == 0 && info.st_size > 0 && info.st_size <= static_cast<off_t>(kMaxArtworkTransferBytes)) {
      bytes.resize(static_cast<std::size_t>(info.st_size)); std::size_t at = 0;
      while (at < bytes.size()) {
        const auto count = read(fd,bytes.data()+at,bytes.size()-at);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) { bytes.clear(); break; } at += static_cast<std::size_t>(count);
      }
    }
    close(fd);
    return cancel.load() ? std::nullopt : DecodeArtwork(bytes);
  };
  if (auto cached = read_cached()) return cached;
  if (cancel.load()) return std::nullopt;
  std::optional<std::vector<std::uint8_t>> encoded;
  if (fetch_) encoded = fetch_(key,cancel);
  else {
    if (!http_) http_ = std::make_unique<steam::HttpClient>(ca_bundle_);
    steam::HttpRequest request;
    request.method = steam::HttpMethod::kGet; request.url = key.source;
    request.timeout_ms = kArtworkTimeoutMs; request.max_response_bytes = kMaxArtworkTransferBytes;
    auto response = http_->Perform(request,cancel);
    if (response.ok() && response.status_code == 200) encoded = std::move(response.body);
  }
  if (!encoded || cancel.load()) return std::nullopt;
  auto image = DecodeArtwork(*encoded);
  if (!image || !directory) return image;
  // Cap this disposable cache at 128 MiB and 2048 entries. Failed cache IO
  // never prevents viewing a successfully downloaded image.
  Storage storage(cache_root_); auto listed = storage.ListFiles();
  if (!listed.ok()) return image;
  auto files = std::move(listed.value());
  std::sort(files.begin(),files.end(),[](const auto& a,const auto& b) { return a.modified_ns < b.modified_ns; });
  std::uint64_t total = 0; std::size_t count = 0;
  for (const auto& file : files) {
    struct stat info{};
    if (file.name.ends_with(".image") && directory->InspectChild(file.name,&info) && S_ISREG(info.st_mode)) { total += info.st_size; ++count; }
  }
  for (const auto& file : files) {
    if (total + encoded->size() <= 128 * 1024 * 1024 && count < 2048) break;
    struct stat info{};
    if (file.name.ends_with(".image") && directory->InspectChild(file.name,&info) && S_ISREG(info.st_mode) && directory->RemoveRegularFile(file.name)) {
      total -= info.st_size; --count;
    }
  }
  if (total + encoded->size() > 128 * 1024 * 1024 || count >= 2048) return image;
  const auto temp = name + ".tmp";
  directory->RemoveRegularFile(temp);
  const int fd = directory->CreateExclusiveFile(temp,0600);
  if (fd >= 0) {
    std::size_t at = 0;
    while (at < encoded->size()) {
      const auto written = write(fd,encoded->data()+at,encoded->size()-at);
      if (written < 0 && errno == EINTR) continue;
      if (written <= 0) break; at += static_cast<std::size_t>(written);
    }
    const bool ready = at == encoded->size() && fsync(fd) == 0; close(fd);
    if (ready && directory->RenameChild(temp,*directory,name)) directory->Sync();
    else directory->RemoveRegularFile(temp);
  }
  return image;
}

}  // namespace playden::homebrew
