// stub.cpp — compiled in place of qnn_server.cpp when no QAIRT SDK
// (QNN_SDK_ROOT) is available at build time. Keeps `virtues-qnnd` building on
// dev machines and non-Dragon CI legs. Running it is a configuration error: a
// real NPU daemon is only produced where the QAIRT SDK is present.
#include <cstdio>

extern "C" int qnnd_main(int, char**) {
  fprintf(stderr,
          "virtues-qnnd was built WITHOUT the Qualcomm QAIRT SDK "
          "(QNN_SDK_ROOT unset at build time), so it cannot drive the Hexagon "
          "NPU. Rebuild on the Dragon (or a leg with the SDK) with "
          "QNN_SDK_ROOT pointing at a QAIRT 2.42 install.\n");
  return 2;
}
