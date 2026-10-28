// title_icon.cpp - A game's PS5 title icon from its store cover art.
// Copyright (c) 2026 Playden contributors
// SPDX-License-Identifier: MIT
#include "homebrew/title_icon.hpp"

#include <algorithm>
#include <cstdint>
#include <vector>
#include <array>

// A private copy of the PNG writer: the app's renderer has its own.
#define STB_IMAGE_WRITE_STATIC
#define STB_IMAGE_WRITE_IMPLEMENTATION
#include "stb_image_write.h"

namespace playden::homebrew {

std::optional<std::string> MakeTitleIcon(const ArtworkImage& cover) {
  if (cover.width == 0 || cover.height == 0 ||
      cover.rgba.size() != static_cast<std::size_t>(cover.width) * cover.height * 4)
    return std::nullopt;
  const std::uint32_t side = std::min(cover.width, cover.height);
  const std::uint32_t left = (cover.width - side) / 2, top = (cover.height - side) / 2;
  std::vector<std::uint8_t> pixels(static_cast<std::size_t>(kTitleIconSize) * kTitleIconSize * 3);
  for (std::uint32_t y = 0; y < kTitleIconSize; ++y) {
    const std::uint32_t y0 = top + y * side / kTitleIconSize;
    const std::uint32_t y1 = std::max(y0 + 1, top + (y + 1) * side / kTitleIconSize);
    for (std::uint32_t x = 0; x < kTitleIconSize; ++x) {
      const std::uint32_t x0 = left + x * side / kTitleIconSize;
      const std::uint32_t x1 = std::max(x0 + 1, left + (x + 1) * side / kTitleIconSize);
      std::uint64_t sum[3] = {0, 0, 0};
      for (std::uint32_t sy = y0; sy < y1; ++sy)
        for (std::uint32_t sx = x0; sx < x1; ++sx) {
          const auto* p = &cover.rgba[(static_cast<std::size_t>(sy) * cover.width + sx) * 4];
          // Transparent areas become black, as the console shows icons on dark tiles.
          for (int c = 0; c < 3; ++c) sum[c] += static_cast<std::uint64_t>(p[c]) * p[3] / 255;
        }
      const std::uint64_t count = static_cast<std::uint64_t>(y1 - y0) * (x1 - x0);
      auto* out = &pixels[(static_cast<std::size_t>(y) * kTitleIconSize + x) * 3];
      for (int c = 0; c < 3; ++c) out[c] = static_cast<std::uint8_t>((sum[c] + count / 2) / count);
    }
  }
  std::string png;
  const auto append = [](void* context, void* data, int size) {
    static_cast<std::string*>(context)->append(static_cast<const char*>(data), static_cast<std::size_t>(size));
  };
  if (!stbi_write_png_to_func(append, &png, kTitleIconSize, kTitleIconSize, 3, pixels.data(),
                              static_cast<int>(kTitleIconSize * 3)) || png.empty())
    return std::nullopt;
  return png;
}

std::optional<std::string> MakeTitlePicture(const ArtworkImage& artwork) {
  if (!artwork.width || !artwork.height ||
      artwork.rgba.size() != static_cast<std::size_t>(artwork.width) * artwork.height * 4) return {};
  constexpr std::uint32_t width = 1920, height = 1080;
  std::string dds(128 + width * height / 2, '\0');
  const auto word = [&](std::size_t offset, std::uint32_t value) {
    for (unsigned b=0;b<4;++b) dds[offset+b] = static_cast<char>(value >> (b*8));
  };
  dds.replace(0,4,"DDS "); word(4,124); word(8,0x81007); word(12,height); word(16,width);
  word(20,width*height/2); word(76,32); word(80,4); word(84,0x31545844); word(108,0x1000);
  const double fill = std::max(double(width)/artwork.width,double(height)/artwork.height);
  const auto sample = [&](unsigned x,unsigned y) -> std::array<unsigned,3> {
    const double sx = (x+0.5-width/2.0)/fill+artwork.width/2.0;
    const double sy = (y+0.5-height/2.0)/fill+artwork.height/2.0;
    const auto ix = std::clamp(static_cast<int>(sx),0,static_cast<int>(artwork.width)-1);
    const auto iy = std::clamp(static_cast<int>(sy),0,static_cast<int>(artwork.height)-1);
    const auto* p = &artwork.rgba[(static_cast<std::size_t>(iy)*artwork.width+ix)*4];
    std::array<unsigned,3> color{};
    for (int c=0;c<3;++c) color[c] = unsigned(p[c])*p[3]/255;
    return color;
  };
  const auto pack = [](const std::array<unsigned,3>& c) -> std::uint16_t {
    return static_cast<std::uint16_t>((c[0]>>3)<<11 | (c[1]>>2)<<5 | (c[2]>>3));
  };
  const auto unpack = [](std::uint16_t c) -> std::array<unsigned,3> {
    return {unsigned((c>>11)&31)*255/31,unsigned((c>>5)&63)*255/63,unsigned(c&31)*255/31};
  };
  std::size_t offset = 128;
  for (unsigned y=0;y<height;y+=4) for (unsigned x=0;x<width;x+=4) {
    std::array<std::array<unsigned,3>,16> pixels{};
    std::array<unsigned,3> lo{255,255,255}, hi{};
    for (unsigned i=0;i<16;++i) {
      pixels[i]=sample(x+i%4,y+i/4);
      for (int c=0;c<3;++c) {lo[c]=std::min(lo[c],pixels[i][c]);hi[c]=std::max(hi[c],pixels[i][c]);}
    }
    const auto a=pack(hi), b=pack(lo);
    std::array<std::array<unsigned,3>,4> palette{unpack(a),unpack(b),{}, {}};
    for (int c=0;c<3;++c) {palette[2][c]=(2*palette[0][c]+palette[1][c])/3;palette[3][c]=(palette[0][c]+2*palette[1][c])/3;}
    std::uint32_t indices=0;
    for (unsigned i=0;i<16;++i) {
      unsigned chosen=0, best=~0U;
      for (unsigned p=0;p<(a==b ? 1U : 4U);++p) {
        unsigned distance=0;
        for (int c=0;c<3;++c) {int d=int(pixels[i][c])-int(palette[p][c]);distance+=d*d;}
        if (distance<best) {best=distance;chosen=p;}
      }
      indices |= chosen<<(i*2);
    }
    word(offset,std::uint32_t(a)|(std::uint32_t(b)<<16));word(offset+4,indices);offset+=8;
  }
  return dds;
}

}  // namespace playden::homebrew
