{-# LANGUAGE RankNTypes #-}
-- | Stub runtime for cubical primitives emitted by the uwuc transpiler.
-- |
-- | These definitions are intentionally incomplete: they provide names and
-- | types so generated Haskell can be type-checked, but cubical computation
-- | is not implemented.
module Cubical.Prelude where

import Prelude hiding (id)

data I

i0, i1 :: I
i0 = undefined
i1 = undefined

iType :: I
iType = undefined

type Path a u v = a

path :: a -> u -> v -> Path a u v
path _ u _ = u

plam :: (I -> a) -> Path a a a
plam f = f i0

papp :: Path a u v -> I -> a
papp _ _ = undefined

hcomp :: a -> I -> a -> a -> a
hcomp _ _ _ base = base

type Equiv a b = a -> b

equivType :: a -> b -> Equiv a b
equivType _ _ = undefined

mkEquiv :: a -> b -> (a -> b) -> (b -> a) -> (a -> b -> a) -> (b -> a -> b) -> Equiv a b
mkEquiv _ _ f _ _ _ = f

equivFwd :: Equiv a b -> a -> b
equivFwd f = f

ua :: Equiv a b -> Path (a -> a) (b -> b)
ua _ = undefined

transport :: Path (a -> a) a -> a -> a
transport _ x = x

type Glue a phi te = a

glueType :: a -> I -> te -> Glue a phi te
glueType a _ _ = a

glueElem :: I -> a -> a -> Glue a I a
glueElem _ t _ = t

unglue :: I -> te -> Glue a phi te -> a
unglue _ _ g = g

infixl 7 /\
infixl 6 \/

(/\) :: I -> I -> I
(/\) = undefined

(\/) :: I -> I -> I
(\/) = undefined

infix 8 ~

(~) :: I -> I
(~) = undefined

infixl 8 @

(@) :: Path a u v -> I -> a
(@) = papp
