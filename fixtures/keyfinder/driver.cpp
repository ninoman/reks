// Reference driver for the libKeyFinder parity test (phase 3 task 5).
// Reads mono little-endian f32 PCM on stdin, prints libKeyFinder's key_t
// (0 = A major … 23 = A flat minor, 24 = silence) on stdout.
//
// Build (needs libkeyfinder >= 2.2 and pkg-config):
//   c++ -std=c++17 -O2 fixtures/keyfinder/driver.cpp \
//       $(pkg-config --cflags --libs libkeyfinder) -o target/keyfinder-driver
// Use:
//   REKS_KEYFINDER_DRIVER=$PWD/target/keyfinder-driver \
//       cargo test -p reks-analysis --test key -- --ignored
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <vector>

#include <keyfinder/keyfinder.h>

int main(int argc, char** argv) {
  if (argc != 2) {
    std::cerr << "usage: keyfinder-driver <sample-rate> < mono-f32le.raw\n";
    return 2;
  }
  unsigned int rate = static_cast<unsigned int>(std::strtoul(argv[1], nullptr, 10));
  std::vector<float> pcm;
  float buf[4096];
  size_t n;
  while ((n = std::fread(buf, sizeof(float), 4096, stdin)) > 0) {
    pcm.insert(pcm.end(), buf, buf + n);
  }
  try {
    KeyFinder::AudioData audio;
    audio.setFrameRate(rate);
    audio.setChannels(1);
    audio.addToSampleCount(static_cast<unsigned int>(pcm.size()));
    for (size_t i = 0; i < pcm.size(); i++) {
      audio.setSample(static_cast<unsigned int>(i), pcm[i]);
    }
    KeyFinder::KeyFinder finder;
    std::cout << static_cast<int>(finder.keyOfAudio(audio)) << "\n";
  } catch (const std::exception& e) {
    std::cerr << e.what() << "\n";
    return 1;
  }
  return 0;
}
