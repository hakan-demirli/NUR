#include <cassert>
#include <cstdlib>
#include <filesystem>
#include <optional>
#include <stdexcept>

#include "src/cli/serve/cache_directory.hpp"

int main() {
  using gufo::cli::ResolveDiskCacheDirectory;
  assert(setenv("HOME", "/home/test user", 1) == 0);
  assert(setenv("XDG_CACHE_HOME", "/var/cache/test user", 1) == 0);
  assert(ResolveDiskCacheDirectory(std::nullopt, 8080) ==
         "/var/cache/test user/gufo/server-8080");
  assert(ResolveDiskCacheDirectory(std::nullopt, 8081) ==
         "/var/cache/test user/gufo/server-8081");

  assert(setenv("XDG_CACHE_HOME", "relative-cache", 1) == 0);
  assert(ResolveDiskCacheDirectory(std::nullopt, 8080) ==
         "/home/test user/.cache/gufo/server-8080");
  assert(unsetenv("XDG_CACHE_HOME") == 0);
  assert(ResolveDiskCacheDirectory(std::nullopt, 8080) ==
         "/home/test user/.cache/gufo/server-8080");

  assert(unsetenv("HOME") == 0);
  assert(ResolveDiskCacheDirectory(std::filesystem::path("/srv/cache"), 8080) ==
         "/srv/cache");
  assert(ResolveDiskCacheDirectory(std::filesystem::path{}, 8080).empty());

  bool rejected = false;
  try {
    (void)ResolveDiskCacheDirectory(std::nullopt, 8080);
  } catch (const std::runtime_error&) {
    rejected = true;
  }
  assert(rejected);
}
