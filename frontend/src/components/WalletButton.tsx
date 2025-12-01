import { FC } from 'react';
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';

export const WalletButton: FC = () => {
  return (
    <WalletMultiButton 
      className="!bg-gradient-to-r !from-purple-600 !to-blue-600 hover:!from-purple-700 hover:!to-blue-700 !transition-all !duration-200 !font-mono-tech !text-sm !uppercase !tracking-wider"
    />
  );
};

export default WalletButton;
