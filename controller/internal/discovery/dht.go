package discovery

//
//
// func SeedDHT(ctx context.Context, idht *IpfsDHT, targets []peer.ID)
// 	slots := make(chan struct{}, 4)
// 	var workers sync.WaitGroup
//
// 	for _, target := range targets {
// 		workers.Add(1)
//
// 		go func(id peer.ID) {
// 			defer workers.Done()
//
// 			select {
// 			case slots <- struct{}{}:
// 				defer func() { <-slots }()
// 			case <-ctx.Done():
// 				return
// 			}
//
// 			lookupCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
// 			defer cancel()
//
// 			info, err := findEventually(lookupCtx, idht, id)
// 			if err != nil {
// 				slog.Error(fmt.Sprintf("lookup failed for %s: %v", id, err))
// 				return
// 			}
//
// 			slog.Info(fmt.Sprintf("resolved %s: %v", info.ID, info.Addrs))
// 		}(target)
// 	}
//
// 	workers.Wait()
//
// 	slog.Info("seed discovery running")
// 	<-ctx.Done()
