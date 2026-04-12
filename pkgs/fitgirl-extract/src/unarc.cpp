#include <iostream>
#include <windows.h>

using namespace std;

typedef unsigned int(__stdcall *f_cb)(const char *, int, int, const char *);
typedef int(__cdecl *f_unarc)(f_cb, const char *, const char *, const char *,
                              const char *, const char *, const char *,
                              const char *, const char *, const char *,
                              const char *);

unsigned int __stdcall cb(const char *a, int b, int c, const char *d) {
  cout << (char *)a << " " << b << " " << c << " " << (char *)d << endl;
  return 0;
}

int main(int argc, char **argv) {
  if (argc < 8) {
    cout << "usage: " << argv[0]
         << " <unarc.dll> <cmd> <opt1> <opt2> <opt3> <opt4> <archive>" << endl;
    cout << "example: " << argv[0]
         << " unarc.dll x -o+ -dpout -wtmp -cfgarc.ini fg-01.bin" << endl;
    return 1;
  }

  HINSTANCE h = LoadLibraryA(argv[1]);
  if (!h) {
    cout << "ERROR: could not load " << argv[1] << endl;
    return 1;
  }

  f_unarc unarc = (f_unarc)GetProcAddress(h, "FreeArcExtract");
  if (!unarc) {
    cout << "ERROR: FreeArcExtract not found in " << argv[1] << endl;
    return 1;
  }

  const char *n = "";
  int ret = unarc(&cb, argv[2], argv[3], argv[4], argv[5], argv[6], argv[7],
                  argc > 8 ? argv[8] : n, argc > 9 ? argv[9] : n, n, n);

  cout << "Exit code: " << ret << endl;
  return ret;
}
