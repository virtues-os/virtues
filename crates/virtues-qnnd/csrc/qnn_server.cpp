// qnn_server.cpp — persistent QNN HTP serving daemon for the Dragon (QCS6490 / Hexagon v68)
// Loads context binaries ONCE, serves executes over TCP. Protocol (little-endian):
//   request : u32 model_idx | u32 payload_bytes | payload (concatenated raw input tensors, native dtype)
//   response: u32 status(0=ok) | u32 payload_bytes | payload (concatenated fp32 outputs, dequantized)
//
// This is virtues' own code; it builds against the Qualcomm QAIRT SDK headers
// (Confidential/Proprietary — NOT vendored). The entry point is exposed as
// `extern "C" qnnd_main` so the `virtues-qnnd` cargo crate can link it behind a
// thin Rust `main` (see build.rs / src/main.rs). The QNN runtime libs
// (libQnnHtp.so, libQnnSystem.so) are dlopen'd at runtime, so only the SDK
// headers are needed at build time.
//   Standalone build: g++ -O2 -std=c++17 qnn_server.cpp -o qnn_server -ldl \
//                       -I$QNN_SDK_ROOT/include -I$QNN_SDK_ROOT/include/QNN
#include <arpa/inet.h>
#include <dlfcn.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sys/socket.h>
#include <unistd.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <chrono>
#include <string>
#include <vector>

#include <mutex>
#include <thread>
#include "QNN/QnnInterface.h"
#include "QNN/System/QnnSystemInterface.h"
#include "QNN/HTP/QnnHtpDevice.h"
#include "QNN/HTP/QnnHtpPerfInfrastructure.h"

#define DIE(...) do { fprintf(stderr, "FATAL: " __VA_ARGS__); fprintf(stderr, "\n"); exit(1); } while (0)
#define CHECK(err, what) do { Qnn_ErrorHandle_t e_ = (err); if (e_ != QNN_SUCCESS) DIE("%s failed: 0x%lx", what, (unsigned long)e_); } while (0)

typedef Qnn_ErrorHandle_t (*GetProvidersFn)(const QnnInterface_t***, uint32_t*);
typedef Qnn_ErrorHandle_t (*GetSysProvidersFn)(const QnnSystemInterface_t***, uint32_t*);

// ---- tensor accessors (v1/v2 tolerant) ----
static uint32_t tid(const Qnn_Tensor_t& t){ return t.version==QNN_TENSOR_VERSION_1? t.v1.id : t.v2.id; }
static const char* tname(const Qnn_Tensor_t& t){ return t.version==QNN_TENSOR_VERSION_1? t.v1.name : t.v2.name; }
static Qnn_DataType_t tdtype(const Qnn_Tensor_t& t){ return t.version==QNN_TENSOR_VERSION_1? t.v1.dataType : t.v2.dataType; }
static uint32_t trank(const Qnn_Tensor_t& t){ return t.version==QNN_TENSOR_VERSION_1? t.v1.rank : t.v2.rank; }
static uint32_t* tdims(const Qnn_Tensor_t& t){ return t.version==QNN_TENSOR_VERSION_1? t.v1.dimensions : t.v2.dimensions; }
static Qnn_QuantizeParams_t tquant(const Qnn_Tensor_t& t){ return t.version==QNN_TENSOR_VERSION_1? t.v1.quantizeParams : t.v2.quantizeParams; }

static size_t dtypeSize(Qnn_DataType_t d){
  switch(d){
    case QNN_DATATYPE_INT_8: case QNN_DATATYPE_UINT_8:
    case QNN_DATATYPE_SFIXED_POINT_8: case QNN_DATATYPE_UFIXED_POINT_8: case QNN_DATATYPE_BOOL_8: return 1;
    case QNN_DATATYPE_INT_16: case QNN_DATATYPE_UINT_16: case QNN_DATATYPE_FLOAT_16:
    case QNN_DATATYPE_SFIXED_POINT_16: case QNN_DATATYPE_UFIXED_POINT_16: return 2;
    case QNN_DATATYPE_INT_64: case QNN_DATATYPE_UINT_64: return 8;
    default: return 4;
  }
}

struct IOTensor {
  Qnn_Tensor_t desc;            // template descriptor (id/name/dtype/rank/dims/quant)
  std::vector<uint32_t> dims;
  std::string name;
  size_t numElems=1, byteSize=0;
  Qnn_DataType_t dt;
  float scale=1.f; int32_t offset=0; bool quantized=false;
};

struct Model {
  std::string binPath, graphName;
  Qnn_ContextHandle_t ctx=nullptr;
  Qnn_GraphHandle_t graph=nullptr;
  std::vector<IOTensor> ins, outs;
  std::vector<std::vector<uint8_t>> inBufs, outBufs;   // native buffers
  std::vector<float> outF32;                            // dequantized concat
  std::mutex mtx;
  size_t inTotal=0, outElems=0;
};

static QNN_INTERFACE_VER_TYPE qnn;
static Qnn_BackendHandle_t backend=nullptr;
static Qnn_DeviceHandle_t device=nullptr;

static IOTensor makeIO(const Qnn_Tensor_t& src, bool isInput){
  IOTensor io; io.desc = src;
  io.name = tname(src);
  io.dt = tdtype(src);
  uint32_t r = trank(src); uint32_t* d = tdims(src);
  for (uint32_t i=0;i<r;i++){ io.dims.push_back(d[i]); io.numElems *= d[i]; }
  io.byteSize = io.numElems * dtypeSize(io.dt);
  Qnn_QuantizeParams_t q = tquant(src);
  if (q.encodingDefinition == QNN_DEFINITION_DEFINED &&
      q.quantizationEncoding == QNN_QUANTIZATION_ENCODING_SCALE_OFFSET){
    io.quantized = true; io.scale = q.scaleOffsetEncoding.scale; io.offset = q.scaleOffsetEncoding.offset;
  }
  // repoint descriptor at owned dims + set client-facing fields
  if (io.desc.version==QNN_TENSOR_VERSION_1){
    io.desc.v1.dimensions = io.dims.data();
    io.desc.v1.memType = QNN_TENSORMEMTYPE_RAW;
    io.desc.v1.type = isInput? QNN_TENSOR_TYPE_APP_WRITE : QNN_TENSOR_TYPE_APP_READ;
  } else {
    io.desc.v2.dimensions = io.dims.data();
    io.desc.v2.memType = QNN_TENSORMEMTYPE_RAW;
    io.desc.v2.type = isInput? QNN_TENSOR_TYPE_APP_WRITE : QNN_TENSOR_TYPE_APP_READ;
  }
  return io;
}

static void setClientBuf(Qnn_Tensor_t& t, void* p, uint32_t n){
  if (t.version==QNN_TENSOR_VERSION_1){ t.v1.clientBuf.data=p; t.v1.clientBuf.dataSize=n; }
  else { t.v2.clientBuf.data=p; t.v2.clientBuf.dataSize=n; }
}

static bool loadModel(Model& m, const QNN_SYSTEM_INTERFACE_VER_TYPE& sysIf){
  FILE* f=fopen(m.binPath.c_str(),"rb"); if(!f) DIE("open %s", m.binPath.c_str());
  fseek(f,0,SEEK_END); long sz=ftell(f); fseek(f,0,SEEK_SET);
  std::vector<uint8_t> buf(sz); if(fread(buf.data(),1,sz,f)!=(size_t)sz) DIE("read %s", m.binPath.c_str()); fclose(f);

  // metadata via system context
  QnnSystemContext_Handle_t sc=nullptr;
  CHECK(sysIf.systemContextCreate(&sc), "systemContextCreate");
  const QnnSystemContext_BinaryInfo_t* info=nullptr; Qnn_ContextBinarySize_t infoSz=0;
  CHECK(sysIf.systemContextGetBinaryInfo(sc, buf.data(), sz, &info, &infoSz), "getBinaryInfo");
  uint32_t numGraphs=0; const QnnSystemContext_GraphInfo_t* graphs=nullptr;
  if (info->version==QNN_SYSTEM_CONTEXT_BINARY_INFO_VERSION_1){ numGraphs=info->contextBinaryInfoV1.numGraphs; graphs=info->contextBinaryInfoV1.graphs; }
  else if (info->version==QNN_SYSTEM_CONTEXT_BINARY_INFO_VERSION_2){ numGraphs=info->contextBinaryInfoV2.numGraphs; graphs=info->contextBinaryInfoV2.graphs; }
  else if (info->version==QNN_SYSTEM_CONTEXT_BINARY_INFO_VERSION_3){ numGraphs=info->contextBinaryInfoV3.numGraphs; graphs=info->contextBinaryInfoV3.graphs; }
  if (!numGraphs) DIE("no graphs in %s", m.binPath.c_str());
  const QnnSystemContext_GraphInfo_t& g0=graphs[0];
  const char* gname=nullptr; uint32_t nIn=0,nOut=0; Qnn_Tensor_t* gin=nullptr; Qnn_Tensor_t* gout=nullptr;
  if (g0.version==QNN_SYSTEM_CONTEXT_GRAPH_INFO_VERSION_1){ gname=g0.graphInfoV1.graphName; nIn=g0.graphInfoV1.numGraphInputs; gin=g0.graphInfoV1.graphInputs; nOut=g0.graphInfoV1.numGraphOutputs; gout=g0.graphInfoV1.graphOutputs; }
  else if (g0.version==QNN_SYSTEM_CONTEXT_GRAPH_INFO_VERSION_2){ gname=g0.graphInfoV2.graphName; nIn=g0.graphInfoV2.numGraphInputs; gin=g0.graphInfoV2.graphInputs; nOut=g0.graphInfoV2.numGraphOutputs; gout=g0.graphInfoV2.graphOutputs; }
  else if (g0.version==QNN_SYSTEM_CONTEXT_GRAPH_INFO_VERSION_3){ gname=g0.graphInfoV3.graphName; nIn=g0.graphInfoV3.numGraphInputs; gin=g0.graphInfoV3.graphInputs; nOut=g0.graphInfoV3.numGraphOutputs; gout=g0.graphInfoV3.graphOutputs; }
  m.graphName = gname? gname:"";
  for(uint32_t i=0;i<nIn;i++)  m.ins.push_back(makeIO(gin[i], true));
  for(uint32_t i=0;i<nOut;i++) m.outs.push_back(makeIO(gout[i], false));
  sysIf.systemContextFree(sc);   // note: invalidates info->strings; name copied already

  // live context
  CHECK(qnn.contextCreateFromBinary(backend, device, nullptr, buf.data(), sz, &m.ctx, nullptr), "contextCreateFromBinary");
  CHECK(qnn.graphRetrieve(m.ctx, m.graphName.c_str(), &m.graph), "graphRetrieve");

  size_t totalOut=0;
  for(auto& io:m.ins){ m.inBufs.emplace_back(io.byteSize); m.inTotal+=io.byteSize; }
  for(auto& io:m.outs){ m.outBufs.emplace_back(io.byteSize); totalOut+=io.numElems; }
  m.outF32.resize(totalOut); m.outElems=totalOut;
  fprintf(stderr,"[loaded] %s graph=%s", m.binPath.c_str(), m.graphName.c_str());
  for(auto& io:m.ins)  fprintf(stderr,"  in:%s dt=0x%x %zuB", io.name.c_str(), io.dt, io.byteSize);
  for(auto& io:m.outs) fprintf(stderr,"  out:%s dt=0x%x %zuB q=%d s=%g o=%d", io.name.c_str(), io.dt, io.byteSize, io.quantized, io.scale, io.offset);
  fprintf(stderr,"\n");
  return true;
}

static int execModel(Model& m, const uint8_t* payload, size_t nbytes){
  size_t need=0; for(auto& io:m.ins) need+=io.byteSize;
  if (nbytes!=need){ fprintf(stderr,"bad payload %zu != %zu\n", nbytes, need); return -1; }
  std::vector<Qnn_Tensor_t> tin, tout;
  size_t off=0;
  for(size_t i=0;i<m.ins.size();i++){
    memcpy(m.inBufs[i].data(), payload+off, m.ins[i].byteSize); off+=m.ins[i].byteSize;
    Qnn_Tensor_t t=m.ins[i].desc; setClientBuf(t, m.inBufs[i].data(), m.ins[i].byteSize); tin.push_back(t);
  }
  for(size_t i=0;i<m.outs.size();i++){
    Qnn_Tensor_t t=m.outs[i].desc; setClientBuf(t, m.outBufs[i].data(), m.outs[i].byteSize); tout.push_back(t);
  }
  Qnn_ErrorHandle_t e=qnn.graphExecute(m.graph, tin.data(), tin.size(), tout.data(), tout.size(), nullptr, nullptr);
  if (e!=QNN_SUCCESS){ fprintf(stderr,"graphExecute 0x%lx\n",(unsigned long)e); return -2; }
  // dequantize/convert outputs to fp32
  float* dst=m.outF32.data();
  for(size_t i=0;i<m.outs.size();i++){
    IOTensor& io=m.outs[i]; const uint8_t* src=m.outBufs[i].data();
    switch(io.dt){
      case QNN_DATATYPE_FLOAT_32: memcpy(dst, src, io.numElems*4); break;
      case QNN_DATATYPE_UFIXED_POINT_16: { const uint16_t* p=(const uint16_t*)src; for(size_t j=0;j<io.numElems;j++) dst[j]=io.scale*((int32_t)p[j]+io.offset); } break;
      case QNN_DATATYPE_SFIXED_POINT_16: { const int16_t* p=(const int16_t*)src; for(size_t j=0;j<io.numElems;j++) dst[j]=io.scale*((int32_t)p[j]+io.offset); } break;
      case QNN_DATATYPE_UFIXED_POINT_8: { const uint8_t* p=src; for(size_t j=0;j<io.numElems;j++) dst[j]=io.scale*((int32_t)p[j]+io.offset); } break;
      case QNN_DATATYPE_SFIXED_POINT_8: { const int8_t* p=(const int8_t*)src; for(size_t j=0;j<io.numElems;j++) dst[j]=io.scale*((int32_t)p[j]+io.offset); } break;
      case QNN_DATATYPE_INT_32: { const int32_t* p=(const int32_t*)src; for(size_t j=0;j<io.numElems;j++) dst[j]=(float)p[j]; } break;
      default: fprintf(stderr,"unhandled out dtype 0x%x\n", io.dt); return -3;
    }
    dst += io.numElems;
  }
  return 0;
}

static void enableBurst(){
  if(!qnn.deviceGetInfrastructure){ fprintf(stderr,"[burst] no getInfrastructure\n"); return; }
  QnnDevice_Infrastructure_t di=nullptr;
  if(qnn.deviceGetInfrastructure(&di)!=QNN_SUCCESS||!di){ fprintf(stderr,"[burst] getInfrastructure failed\n"); return; }
  auto* hi=reinterpret_cast<QnnHtpDevice_Infrastructure_t*>(di);
  if(hi->infraType!=QNN_HTP_DEVICE_INFRASTRUCTURE_TYPE_PERF){ fprintf(stderr,"[burst] not perf infra\n"); return; }
  QnnHtpDevice_PerfInfrastructure_t p=hi->perfInfra;
  uint32_t pcid=0;
  if(p.createPowerConfigId(0,0,&pcid)!=QNN_SUCCESS){ fprintf(stderr,"[burst] createPowerConfigId failed\n"); return; }
  QnnHtpPerfInfrastructure_PowerConfig_t cfg; memset(&cfg,0,sizeof cfg);
  cfg.option=QNN_HTP_PERF_INFRASTRUCTURE_POWER_CONFIGOPTION_DCVS_V3;
  auto& d=cfg.dcvsV3Config;
  d.contextId=pcid; d.setDcvsEnable=1; d.dcvsEnable=0;
  d.powerMode=QNN_HTP_PERF_INFRASTRUCTURE_POWERMODE_PERFORMANCE_MODE;
  d.setSleepLatency=1; d.sleepLatency=40;
  d.setSleepDisable=0; d.sleepDisable=0;
  d.setBusParams=1; d.busVoltageCornerMin=DCVS_VOLTAGE_VCORNER_TURBO_PLUS; d.busVoltageCornerTarget=DCVS_VOLTAGE_VCORNER_TURBO_PLUS; d.busVoltageCornerMax=DCVS_VOLTAGE_VCORNER_TURBO_PLUS;
  d.setCoreParams=1; d.coreVoltageCornerMin=DCVS_VOLTAGE_VCORNER_TURBO_PLUS; d.coreVoltageCornerTarget=DCVS_VOLTAGE_VCORNER_TURBO_PLUS; d.coreVoltageCornerMax=DCVS_VOLTAGE_VCORNER_TURBO_PLUS;
  const QnnHtpPerfInfrastructure_PowerConfig_t* cfgs[]={&cfg,nullptr};
  if(p.setPowerConfig(pcid,cfgs)!=QNN_SUCCESS) fprintf(stderr,"[burst] setPowerConfig failed\n");
  else fprintf(stderr,"[burst] HTP performance mode ON (turbo+ corners)\n");
}

static bool readN(int fd, void* p, size_t n){ uint8_t* b=(uint8_t*)p; while(n){ ssize_t r=read(fd,b,n); if(r<=0) return false; b+=r; n-=r; } return true; }
static bool writeN(int fd, const void* p, size_t n){ const uint8_t* b=(const uint8_t*)p; while(n){ ssize_t r=write(fd,b,n); if(r<=0) return false; b+=r; n-=r; } return true; }

extern "C" int qnnd_main(int argc, char** argv){
  if (argc<2){ fprintf(stderr,"usage: %s model1.bin [model2.bin ...] [--port N]\n", argv[0]); return 1; }
  int port=7788; bool burst=false;
  std::vector<std::string> bins;
  for(int i=1;i<argc;i++){ if(!strcmp(argv[i],"--port")&&i+1<argc){ port=atoi(argv[++i]); } else if(!strcmp(argv[i],"--burst")){ burst=true; } else bins.push_back(argv[i]); }

  // ---- backend init (once) ----
  void* h=dlopen("libQnnHtp.so", RTLD_NOW|RTLD_GLOBAL); if(!h) DIE("dlopen libQnnHtp.so: %s", dlerror());
  auto getProviders=(GetProvidersFn)dlsym(h,"QnnInterface_getProviders"); if(!getProviders) DIE("no QnnInterface_getProviders");
  const QnnInterface_t** provs=nullptr; uint32_t np=0;
  CHECK(getProviders(&provs,&np),"getProviders"); if(!np) DIE("no providers");
  bool found=false;
  for(uint32_t i=0;i<np;i++){
    if(provs[i]->apiVersion.coreApiVersion.major==QNN_API_VERSION_MAJOR){ qnn=provs[i]->QNN_INTERFACE_VER_NAME; found=true; break; }
  }
  if(!found) DIE("no matching API provider");
  void* hs=dlopen("libQnnSystem.so", RTLD_NOW|RTLD_GLOBAL); if(!hs) DIE("dlopen libQnnSystem.so: %s", dlerror());
  auto getSys=(GetSysProvidersFn)dlsym(hs,"QnnSystemInterface_getProviders"); if(!getSys) DIE("no sys providers fn");
  const QnnSystemInterface_t** sprovs=nullptr; uint32_t nsp=0;
  CHECK(getSys(&sprovs,&nsp),"sysGetProviders"); if(!nsp) DIE("no system providers");
  QNN_SYSTEM_INTERFACE_VER_TYPE sysIf = sprovs[0]->QNN_SYSTEM_INTERFACE_VER_NAME;

  CHECK(qnn.backendCreate(nullptr,nullptr,&backend),"backendCreate");
  if (qnn.deviceCreate){ Qnn_ErrorHandle_t e=qnn.deviceCreate(nullptr,nullptr,&device); if(e!=QNN_SUCCESS){ fprintf(stderr,"deviceCreate 0x%lx (continuing, null device)\n",(unsigned long)e); device=nullptr; } }

  std::vector<Model> models(bins.size());
  for(size_t i=0;i<bins.size();i++){ models[i].binPath=bins[i]; loadModel(models[i], sysIf); }
  if(burst) enableBurst();
  fprintf(stderr,"[ready] %zu models on 127.0.0.1:%d\n", models.size(), port);

  // ---- serve ----
  int srv=socket(AF_INET,SOCK_STREAM,0); int one=1;
  setsockopt(srv,SOL_SOCKET,SO_REUSEADDR,&one,sizeof one);
  sockaddr_in a{}; a.sin_family=AF_INET; a.sin_port=htons(port); a.sin_addr.s_addr=htonl(INADDR_LOOPBACK);
  if(bind(srv,(sockaddr*)&a,sizeof a)) DIE("bind");
  if(listen(srv,8)) DIE("listen");
  auto handle=[&models](int c){
    int one=1; setsockopt(c,IPPROTO_TCP,TCP_NODELAY,&one,sizeof one);
    std::vector<uint8_t> payload; std::vector<float> outAll;
    for(;;){
      uint32_t hdr[2];
      if(!readN(c,hdr,8)) break;
      uint32_t idx=hdr[0], nb=hdr[1];
      if(idx>=models.size()||nb>(64u<<20)){ uint32_t bad[2]={1,0}; writeN(c,bad,8); break; }
      payload.resize(nb);
      if(!readN(c,payload.data(),nb)) break;
      Model& m=models[idx];
      if(m.inTotal==0||nb%m.inTotal){ uint32_t bad[2]={2,0}; writeN(c,bad,8); continue; }
      size_t k=nb/m.inTotal;
      outAll.resize(k*m.outElems);
      int rc=0;
      { std::lock_guard<std::mutex> lk(m.mtx);
        for(size_t b=0;b<k&&!rc;b++){
          rc=execModel(m, payload.data()+b*m.inTotal, m.inTotal);
          if(!rc) memcpy(outAll.data()+b*m.outElems, m.outF32.data(), m.outElems*4);
        }
      }
      if(rc){ uint32_t bad[2]={(uint32_t)(-rc),0}; writeN(c,bad,8); continue; }
      uint32_t outB=(uint32_t)(outAll.size()*4);
      uint32_t ok[2]={0,outB};
      if(!writeN(c,ok,8)||!writeN(c,outAll.data(),outB)) break;
    }
    close(c);
  };
  for(;;){
    int c=accept(srv,nullptr,nullptr); if(c<0) continue;
    std::thread(handle,c).detach();
  }
}
