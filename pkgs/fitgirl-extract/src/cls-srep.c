#include <stdio.h>
#include <string.h>
#include <windows.h>

typedef struct {
  HANDLE hIoEvent;
  HANDLE hIoComplete;
  HANDLE hIoMapping;
  LPVOID pIoBuf;

  HANDLE hReadMapping;
  LPVOID pReadBuf;
  DWORD readBufSize;

  HANDLE hWriteMapping;
  LPVOID pWriteBuf;
  DWORD writeBufSize;

  HANDLE hChildProcess;
  HANDLE hChildThread;
  HANDLE hWorkerThread;

  int(__cdecl *callback)(void *ctx, int op, void *buf, int len);
  void *cbCtx;

  LPVOID slot[4];
  BYTE slotState[4];
  DWORD curSlotIdx;
  DWORD slotSize;
  HANDLE hSlotReady;
  HANDLE hSlotFree;
  HANDLE hReaderThread;
  volatile BYTE eofFlag;

  char dllDir[MAX_PATH];
  char baseName[64];
  DWORD instanceIdx;
} Session;

static DWORD WINAPI ReaderProc(LPVOID param) {
  Session *s = (Session *)param;
  DWORD idx = 0;

  for (;;) {
    while (s->slotState[idx] != 0) {
      if (s->eofFlag)
        return 0;
      WaitForSingleObject(s->hSlotFree, INFINITE);
      if (s->eofFlag)
        return 0;
    }

    int n = s->callback(s->cbCtx, 0x1000, s->slot[idx], (int)s->slotSize);
    if (n <= 0) {
      s->slotState[idx] = 3;
      SetEvent(s->hSlotReady);
      return 0;
    }

    s->slotState[idx] = 2;
    SetEvent(s->hSlotReady);
    idx = (idx + 1) & 3;
  }
}

static DWORD WINAPI WorkerProc(LPVOID param) {
  Session *s = (Session *)param;
  DWORD ringIdx = 0;
  DWORD ringOffset = 0;

  for (;;) {
    WaitForSingleObject(s->hIoEvent, INFINITE);
    DWORD *cmd = (DWORD *)s->pIoBuf;
    DWORD opcode = cmd[0];
    DWORD length = cmd[1];

    if (opcode == 1) {
      DWORD wanted = length;
      DWORD copied = 0;
      BYTE *dst = (BYTE *)s->pReadBuf;

      while (wanted > 0) {
        while (s->slotState[ringIdx] < 2) {
          WaitForSingleObject(s->hSlotReady, INFINITE);
        }

        if (s->slotState[ringIdx] == 3) {
          s->eofFlag = 1;
          break;
        }

        DWORD avail = s->slotSize - ringOffset;
        DWORD take = (wanted < avail) ? wanted : avail;
        memcpy(dst + copied, (BYTE *)s->slot[ringIdx] + ringOffset, take);
        copied += take;
        wanted -= take;
        ringOffset += take;

        if (ringOffset >= s->slotSize) {
          s->slotState[ringIdx] = 0;
          SetEvent(s->hSlotFree);
          ringIdx = (ringIdx + 1) & 3;
          ringOffset = 0;
        }
      }

      if (copied > 0) {
        cmd[2] = copied;
      } else {
        cmd[2] = (DWORD)-1;
      }
      SetEvent(s->hIoComplete);

    } else if (opcode == 2) {
      int rc = s->callback(s->cbCtx, 0x1800, s->pWriteBuf, (int)length);
      cmd[2] = (DWORD)rc;
      SetEvent(s->hIoComplete);

    } else if (opcode == 3) {
      cmd[2] = 0;
      SetEvent(s->hIoComplete);
      return length;

    } else {
      cmd[2] = 0;
      SetEvent(s->hIoComplete);
    }
  }
}

__declspec(dllexport) int __cdecl ClsMain(int command, void *cbFunc,
                                          DWORD cbCtx) {
  if (command == 1 || command == 2)
    return 0;
  if (command != 3 && command != 4)
    return -2;

  Session s;
  memset(&s, 0, sizeof(s));
  s.callback = (int(__cdecl *)(void *, int, void *, int))cbFunc;
  s.cbCtx = (void *)cbCtx;

  SECURITY_ATTRIBUTES sa = {sizeof(sa), NULL, TRUE};

  HMODULE hSelf = NULL;
  GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                         GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                     (LPCSTR)&ClsMain, &hSelf);
  if (!hSelf)
    return -1;

  char fullPath[MAX_PATH];
  GetModuleFileNameA(hSelf, fullPath, MAX_PATH);

  char *lastSlash = NULL, *lastDot = NULL;
  for (char *p = fullPath; *p; p++) {
    if (*p == '\\' || *p == '/')
      lastSlash = p;
    if (*p == '.')
      lastDot = p;
  }
  if (lastDot && (!lastSlash || lastDot > lastSlash))
    *lastDot = '\0';
  if (lastSlash) {
    *lastSlash = '\0';
    strncpy(s.dllDir, fullPath, MAX_PATH - 1);
    strncpy(s.baseName, lastSlash + 1, sizeof(s.baseName) - 1);
  } else {
    s.dllDir[0] = '.';
    s.dllDir[1] = '\0';
    strncpy(s.baseName, fullPath, sizeof(s.baseName) - 1);
  }

  char nameBuf[300];
  for (s.instanceIdx = 0; s.instanceIdx < 100; s.instanceIdx++) {
    snprintf(nameBuf, sizeof(nameBuf), "IOEvent_%s%02u", s.baseName,
             (unsigned)s.instanceIdx);
    HANDLE probe = OpenEventA(EVENT_ALL_ACCESS, FALSE, nameBuf);
    if (!probe)
      break;
    CloseHandle(probe);
  }

  snprintf(nameBuf, sizeof(nameBuf), "IOEvent_%s%02u", s.baseName,
           (unsigned)s.instanceIdx);
  s.hIoEvent = CreateEventA(&sa, FALSE, FALSE, nameBuf);
  if (!s.hIoEvent) {
    MessageBoxA(NULL, "Can't create i/o event!", "Error", MB_ICONERROR);
    return -1;
  }

  snprintf(nameBuf, sizeof(nameBuf), "IOCompleteEvent_%s%02u", s.baseName,
           (unsigned)s.instanceIdx);
  s.hIoComplete = CreateEventA(&sa, FALSE, FALSE, nameBuf);
  if (!s.hIoComplete) {
    MessageBoxA(NULL, "Can't create i/o complete event!", "Error",
                MB_ICONERROR);
    return -1;
  }

  snprintf(nameBuf, sizeof(nameBuf), "IO_MapFile_%s%02u", s.baseName,
           (unsigned)s.instanceIdx);
  s.hIoMapping = CreateFileMappingA(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE,
                                    0, 0x100, nameBuf);
  s.pIoBuf = MapViewOfFile(s.hIoMapping, FILE_MAP_ALL_ACCESS, 0, 0, 0x100);
  if (!s.hIoMapping || !s.pIoBuf) {
    MessageBoxA(NULL, "Can't create i/o file mapping!", "Error", MB_ICONERROR);
    return -1;
  }

  BOOL isWow64 = FALSE;
  IsWow64Process(GetCurrentProcess(), &isWow64);
  const char *arch = isWow64 ? "_x64" : "_x86";
  const char *mode = (command == 3) ? "e" : "d";

  char cmdLine[1024];
  snprintf(cmdLine, sizeof(cmdLine), "\"%s\\%s%s.exe\" %s - - -idx=%02u",
           s.dllDir, s.baseName, arch, mode, (unsigned)s.instanceIdx);

  STARTUPINFOA si;
  memset(&si, 0, sizeof(si));
  si.cb = sizeof(si);
  si.dwFlags = STARTF_USESHOWWINDOW;
  si.wShowWindow = SW_HIDE;

  PROCESS_INFORMATION pi;
  memset(&pi, 0, sizeof(pi));

  if (!CreateProcessA(NULL, cmdLine, NULL, NULL, FALSE, DETACHED_PROCESS, NULL,
                      s.dllDir, &si, &pi)) {
    char msg[512];
    snprintf(msg, sizeof(msg), "Failed to start %s%s.exe", s.baseName, arch);
    MessageBoxA(NULL, msg, "Error", MB_ICONERROR);
    return -1;
  }
  s.hChildProcess = pi.hProcess;
  s.hChildThread = pi.hThread;

  if (WaitForSingleObject(s.hIoEvent, 30000) == WAIT_TIMEOUT) {
    MessageBoxA(NULL, "Launched application does not respond!", "Error",
                MB_ICONERROR);
    TerminateProcess(s.hChildProcess, 1);
    goto cleanup;
  }

  {
    DWORD *hs = (DWORD *)s.pIoBuf;
    s.slotSize = hs[0] / 4;
    s.readBufSize = hs[1];
    s.writeBufSize = hs[2];
  }
  SetEvent(s.hIoComplete);

  snprintf(nameBuf, sizeof(nameBuf), "Global\\Read_MapFile_%s%02u", s.baseName,
           (unsigned)s.instanceIdx);
  s.hReadMapping = OpenFileMappingA(FILE_MAP_ALL_ACCESS, FALSE, nameBuf);
  s.pReadBuf =
      MapViewOfFile(s.hReadMapping, FILE_MAP_ALL_ACCESS, 0, 0, s.readBufSize);
  if (!s.hReadMapping || !s.pReadBuf) {
    MessageBoxA(NULL, "Can't open read file mapping!", "Error", MB_ICONERROR);
    TerminateProcess(s.hChildProcess, 1);
    goto cleanup;
  }

  snprintf(nameBuf, sizeof(nameBuf), "Global\\Write_MapFile_%s%02u", s.baseName,
           (unsigned)s.instanceIdx);
  s.hWriteMapping = OpenFileMappingA(FILE_MAP_ALL_ACCESS, FALSE, nameBuf);
  s.pWriteBuf =
      MapViewOfFile(s.hWriteMapping, FILE_MAP_ALL_ACCESS, 0, 0, s.writeBufSize);
  if (!s.hWriteMapping || !s.pWriteBuf) {
    MessageBoxA(NULL, "Can't open write file mapping!", "Error", MB_ICONERROR);
    TerminateProcess(s.hChildProcess, 1);
    goto cleanup;
  }

  DWORD tid;
  s.hWorkerThread = CreateThread(&sa, 0, WorkerProc, &s, 0, &tid);

  for (int i = 0; i < 4; i++) {
    s.slot[i] = VirtualAlloc(NULL, s.slotSize, MEM_COMMIT | MEM_RESERVE,
                             PAGE_READWRITE);
    s.slotState[i] = 0;
  }
  s.hSlotReady = CreateEventA(NULL, FALSE, FALSE, NULL);
  s.hSlotFree = CreateEventA(NULL, FALSE, FALSE, NULL);
  s.curSlotIdx = 0;
  s.eofFlag = 0;

  s.hReaderThread = CreateThread(&sa, 0, ReaderProc, &s, 0, &tid);

  WaitForSingleObject(s.hWorkerThread, INFINITE);

  DWORD result = 0;
  GetExitCodeThread(s.hWorkerThread, &result);

  s.eofFlag = 1;
  SetEvent(s.hSlotFree);
  WaitForSingleObject(s.hReaderThread, 5000);

cleanup:
  if (s.hWorkerThread)
    CloseHandle(s.hWorkerThread);
  if (s.hReaderThread)
    CloseHandle(s.hReaderThread);
  if (s.hChildThread)
    CloseHandle(s.hChildThread);
  if (s.hChildProcess)
    CloseHandle(s.hChildProcess);

  for (int i = 0; i < 4; i++)
    if (s.slot[i])
      VirtualFree(s.slot[i], 0, MEM_RELEASE);

  if (s.hSlotReady)
    CloseHandle(s.hSlotReady);
  if (s.hSlotFree)
    CloseHandle(s.hSlotFree);

  if (s.pIoBuf)
    UnmapViewOfFile(s.pIoBuf);
  if (s.pReadBuf)
    UnmapViewOfFile(s.pReadBuf);
  if (s.pWriteBuf)
    UnmapViewOfFile(s.pWriteBuf);

  if (s.hIoMapping)
    CloseHandle(s.hIoMapping);
  if (s.hReadMapping)
    CloseHandle(s.hReadMapping);
  if (s.hWriteMapping)
    CloseHandle(s.hWriteMapping);
  if (s.hIoEvent)
    CloseHandle(s.hIoEvent);
  if (s.hIoComplete)
    CloseHandle(s.hIoComplete);

  return (int)result;
}

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpReserved) {
  return TRUE;
}
