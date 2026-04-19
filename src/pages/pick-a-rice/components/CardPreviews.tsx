import styles from './CardPreviews.module.css';
import cleanshotImg from '@/assets/figma/cleanshot.png';
import ricePreviewImg from '@/assets/figma/rice-preview.png';
import { DashedFrame } from './DashedFrame';

export function CardPreviews() {
  return (
    <>
      {/* Top preview (168:7235) — mint-backed clipping container with CleanShot screenshot */}
      <div className={styles.topPreview}>
        <div className={styles.topPreviewImageWrap}>
          <img src={cleanshotImg} alt="" className={styles.topPreviewImage} />
        </div>
      </div>

      {/* Bottom preview (168:7237) — rice-preview.png with 50% black overlay */}
      <div className={styles.bottomPreview}>
        <img src={ricePreviewImg} alt="" className={styles.bottomPreviewImage} />
        <div className={styles.bottomPreviewOverlay} />
      </div>

      {/* Dashed yellow selection frame (168:7306) — drawn over the top preview */}
      <DashedFrame />
    </>
  );
}
