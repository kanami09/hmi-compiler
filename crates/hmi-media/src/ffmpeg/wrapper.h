#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/frame.h>
#include <libswscale/swscale.h>

#include <errno.h>
#include <libavutil/error.h>
enum {
    HMI_ERROR_EOF = ((int)AVERROR_EOF),
    HMI_EAGAIN = ((int)AVERROR(EAGAIN)),
    HMI_DISPLAY_MATRIX = ((int)AV_PKT_DATA_DISPLAYMATRIX),
    HMI_ATTACHED_PIC = ((int)AV_DISPOSITION_ATTACHED_PIC),
    HMI_RANGE_JPEG = ((int)AVCOL_RANGE_JPEG),
    HMI_COLOR_BT709 = ((int)AVCOL_SPC_BT709),
    HMI_COLOR_BT470BG = ((int)AVCOL_SPC_BT470BG),
    HMI_COLOR_SMPTE170M = ((int)AVCOL_SPC_SMPTE170M),
    HMI_COLOR_UNSPECIFIED = ((int)AVCOL_SPC_UNSPECIFIED),
    HMI_SWS_BICUBIC = ((int)SWS_BICUBIC),
    HMI_SWS_CS_709 = ((int)SWS_CS_ITU709),
    HMI_SWS_CS_601 = ((int)SWS_CS_ITU601),
    HMI_TRC_PQ = ((int)AVCOL_TRC_SMPTE2084),
    HMI_TRC_HLG = ((int)AVCOL_TRC_ARIB_STD_B67),
    HMI_INTERLACED = ((int)AV_FRAME_FLAG_INTERLACED)
};
