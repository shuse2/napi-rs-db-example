const x = require('./binding');


(async () => {
  let db = x.createDb('./backup');
  try {
    await x.get(db, Buffer.from([0, 1, 1]));
  } catch (error) {

  }
  console.log('1')
  console.time('a')
  const DB_KEY_BLOCKS_ID = 'blocks:id';
  let count = 0;
  await x.getIterator(db, {
    gte: Buffer.from(`${DB_KEY_BLOCKS_ID}:${Buffer.alloc(32, 0).toString('binary')}`),
    lte: Buffer.from(`${DB_KEY_BLOCKS_ID}:${Buffer.alloc(32, 255).toString('binary')}`),
    limit: -1,
    reverse: false,
  }, (err, result) => {
    count += 1;
  });
  console.log('block', { count })
  count = 0;
  const DB_KEY_TRANSACTIONS_ID = 'transactions:id';
  await x.getIterator(db, {
    gte: Buffer.from(`${DB_KEY_TRANSACTIONS_ID}:${Buffer.alloc(32, 0).toString('binary')}`),
    lte: Buffer.from(`${DB_KEY_TRANSACTIONS_ID}:${Buffer.alloc(32, 255).toString('binary')}`),
    limit: -1,
    reverse: false,
  }, (err, result) => {
    count += 1;
  });
  console.log('tx', { count })
  console.timeEnd('a')

})();